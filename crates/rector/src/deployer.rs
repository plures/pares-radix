//! NixOS workload deployer — generates and applies NixOS modules for workloads.

use crate::node::ClusterNode;
use crate::px_parser::PxWorkload;

/// Deploys workloads to NixOS nodes by generating systemd service modules.
pub struct NixDeployer;

impl NixDeployer {
    /// Deploy a workload to a node by generating and applying a NixOS module.
    pub async fn deploy(
        node: &ClusterNode,
        workload: &PxWorkload,
    ) -> Result<DeployResult, DeployError> {
        let module = Self::generate_module(workload);

        if Self::is_local(node) {
            Self::deploy_local(&module, workload).await
        } else {
            Self::deploy_remote(node, &module, workload).await
        }
    }

    /// Generate a NixOS systemd service module from a workload definition.
    fn generate_module(workload: &PxWorkload) -> String {
        format!(
            r#"{{ config, pkgs, ... }}:
{{
  systemd.services."{name}" = {{
    description = "Managed workload: {name}";
    wantedBy = [ "multi-user.target" ];
    after = [ "network-online.target" ];
    wants = [ "network-online.target" ];

    serviceConfig = {{
      Type = "simple";
      Restart = "on-failure";
      RestartSec = 10;
      DynamicUser = true;
      MemoryMax = "{memory}";
      CPUQuota = "{cpu_pct}%";
    }};

    script = ''
      exec {command}
    '';
  }};
}}"#,
            name = workload.name,
            memory = workload.resources.memory.as_deref().unwrap_or("4G"),
            cpu_pct = (workload.resources.cpu.unwrap_or(1.0) * 100.0) as u32,
            command = Self::resolve_image(&workload.image),
        )
    }

    /// Resolve an image reference to an executable command.
    ///
    /// - `"nixos#pares-radix"` → `nix run nixos#pares-radix`
    /// - `"plures-object://sha256:abc"` → content-store fetch (TODO)
    /// - anything else → used as-is
    fn resolve_image(image: &str) -> String {
        if image.starts_with("nixos#") {
            format!("nix run {image}")
        } else if image.starts_with("plures-object://") {
            "# TODO: fetch from plures-object\n      echo 'plures-object not yet implemented'"
                .to_string()
        } else {
            image.to_string()
        }
    }

    fn is_local(node: &ClusterNode) -> bool {
        hostname::get()
            .ok()
            .map(|h| h.to_string_lossy() == node.hostname)
            .unwrap_or(false)
    }

    async fn deploy_local(
        module: &str,
        workload: &PxWorkload,
    ) -> Result<DeployResult, DeployError> {
        let module_path = format!("/etc/nixos/rector/{}.nix", workload.name);
        tokio::fs::create_dir_all("/etc/nixos/rector")
            .await
            .map_err(DeployError::Io)?;
        tokio::fs::write(&module_path, module)
            .await
            .map_err(DeployError::Io)?;

        let output = tokio::process::Command::new("sudo")
            .args(["nixos-rebuild", "switch"])
            .output()
            .await
            .map_err(DeployError::Io)?;

        if output.status.success() {
            Ok(DeployResult::Success(workload.name.clone()))
        } else {
            Err(DeployError::NixRebuildFailed(
                String::from_utf8_lossy(&output.stderr).into(),
            ))
        }
    }

    async fn deploy_remote(
        node: &ClusterNode,
        module: &str,
        workload: &PxWorkload,
    ) -> Result<DeployResult, DeployError> {
        let addr = node.addresses.first().ok_or(DeployError::NoAddress)?;

        let output = tokio::process::Command::new("ssh")
            .args([
                addr.as_str(),
                &format!(
                    "sudo mkdir -p /etc/nixos/rector && echo '{}' | sudo tee /etc/nixos/rector/{}.nix > /dev/null && sudo nixos-rebuild switch",
                    module.replace('\'', "'\\''"),
                    workload.name,
                ),
            ])
            .output()
            .await
            .map_err(DeployError::Io)?;

        if output.status.success() {
            Ok(DeployResult::Success(workload.name.clone()))
        } else {
            Err(DeployError::RemoteDeployFailed(
                String::from_utf8_lossy(&output.stderr).into(),
            ))
        }
    }
}

/// Result of a successful deployment.
#[derive(Debug, Clone)]
pub enum DeployResult {
    /// Workload was deployed and nixos-rebuild succeeded.
    Success(String),
    /// Workload was already running (no-op).
    AlreadyRunning(String),
}

/// Errors that can occur during deployment.
#[derive(Debug, thiserror::Error)]
pub enum DeployError {
    #[error("nixos-rebuild failed: {0}")]
    NixRebuildFailed(String),
    #[error("remote deploy failed: {0}")]
    RemoteDeployFailed(String),
    #[error("node has no addresses")]
    NoAddress,
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::px_parser::{PxWorkload, ResourceSpec};

    fn test_workload() -> PxWorkload {
        PxWorkload {
            name: "test-app".into(),
            image: "nixos#test-app".into(),
            replicas: crate::px_parser::ReplicaSpec::Count(1),
            placement: Default::default(),
            resources: ResourceSpec {
                cpu: Some(2.0),
                memory: Some("4G".into()),
                gpu: None,
            },
            health: None,
            gates: vec![],
            on_failure: Default::default(),
        }
    }

    #[test]
    fn generate_module_contains_service_name() {
        let w = test_workload();
        let module = NixDeployer::generate_module(&w);
        assert!(module.contains("test-app"));
        assert!(module.contains("MemoryMax = \"4G\""));
        assert!(module.contains("CPUQuota = \"200%\""));
        assert!(module.contains("nix run nixos#test-app"));
    }

    #[test]
    fn resolve_image_nixos_flake() {
        assert_eq!(
            NixDeployer::resolve_image("nixos#my-pkg"),
            "nix run nixos#my-pkg"
        );
    }

    #[test]
    fn resolve_image_plures_object() {
        let r = NixDeployer::resolve_image("plures-object://sha256:abc");
        assert!(r.contains("TODO"));
    }

    #[test]
    fn resolve_image_raw_command() {
        assert_eq!(
            NixDeployer::resolve_image("/usr/bin/myapp"),
            "/usr/bin/myapp"
        );
    }
}
