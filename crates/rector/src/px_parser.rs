use serde::{Deserialize, Serialize};

// ── Top-level .px file (TOML format) ──────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PxFile {
    #[serde(default)]
    pub constraint: Vec<PxConstraint>,
    #[serde(default)]
    pub workload: Vec<PxWorkload>,
    #[serde(default)]
    pub rule: Vec<PxRule>,
    #[serde(default)]
    pub node_requirement: Vec<PxNodeRequirement>,
    #[serde(default)]
    pub discovery: Option<DiscoveryConfig>,
}

// Re-export with the logical names the rest of the crate uses.
impl PxFile {
    pub fn constraints(&self) -> &[PxConstraint] {
        &self.constraint
    }
    pub fn workloads(&self) -> &[PxWorkload] {
        &self.workload
    }
    pub fn rules(&self) -> &[PxRule] {
        &self.rule
    }
    pub fn node_requirements(&self) -> &[PxNodeRequirement] {
        &self.node_requirement
    }
}

// ── Constraint ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PxConstraint {
    pub name: String,
    pub check: String,
    pub severity: String,
    pub message: String,
}

// ── Workload ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PxWorkload {
    pub name: String,
    pub image: String,
    #[serde(default = "default_replicas")]
    pub replicas: ReplicaSpec,
    #[serde(default)]
    pub placement: PlacementStrategy,
    #[serde(default)]
    pub resources: ResourceSpec,
    #[serde(default)]
    pub health: Option<HealthCheck>,
    #[serde(default)]
    pub gates: Vec<String>,
    #[serde(default)]
    pub on_failure: FailurePolicy,
}

fn default_replicas() -> ReplicaSpec {
    ReplicaSpec::Count(1)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ReplicaSpec {
    Count(u32),
    Named(String), // "all"
}

impl ReplicaSpec {
    pub fn is_all(&self) -> bool {
        matches!(self, ReplicaSpec::Named(s) if s == "all")
    }
}

impl Default for ReplicaSpec {
    fn default() -> Self {
        ReplicaSpec::Count(1)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum PlacementStrategy {
    #[default]
    PreferIdle,
    Everywhere,
    Spread,
    #[serde(rename = "colocate")]
    Colocate(String),
    #[serde(rename = "pin")]
    Pin(String),
}


#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResourceSpec {
    pub cpu: Option<f32>,
    pub memory: Option<String>,
    pub gpu: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheck {
    pub protocol: String,
    pub target: String,
    #[serde(default = "default_interval")]
    pub interval_secs: u32,
}

fn default_interval() -> u32 {
    30
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailurePolicy {
    Restart { max_attempts: u32 },
    RestartOnDifferentNode { max_attempts: u32 },
    Alert,
    Ignore,
}

impl Default for FailurePolicy {
    fn default() -> Self {
        FailurePolicy::Restart { max_attempts: 3 }
    }
}

// ── Rule ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PxRule {
    pub name: String,
    pub when_condition: String,
    pub action: String,
    #[serde(default)]
    pub max: Option<String>,
    #[serde(default)]
    pub log_message: Option<String>,
}

// ── Node requirement ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PxNodeRequirement {
    pub name: String,
    pub count_check: String,
    pub error: String,
}

// ── Discovery config ──────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DiscoveryConfig {
    #[serde(default)]
    pub direct: Vec<DirectPeer>,
    pub lan: Option<LanConfig>,
    pub hyperswarm: Option<HyperswarmConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectPeer {
    pub address: String,
    pub port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanConfig {
    pub enabled: bool,
    #[serde(default = "default_multicast")]
    pub multicast_group: String,
    #[serde(default = "default_discovery_port")]
    pub port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HyperswarmConfig {
    pub enabled: bool,
    pub topic_key: String,
}

fn default_multicast() -> String {
    "239.255.77.77".into()
}

fn default_discovery_port() -> u16 {
    7700
}

// ── Parsing ───────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("TOML parse error: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Parse a `.px` file (TOML format) from a string.
pub fn parse(input: &str) -> Result<PxFile, ParseError> {
    Ok(toml::from_str(input)?)
}

/// Parse a `.px` file from disk.
pub fn parse_file(path: &str) -> Result<PxFile, ParseError> {
    let content = std::fs::read_to_string(path)?;
    parse(&content)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
[[node_requirement]]
name = "minimum_nodes"
count_check = ">= 3"
error = "Cluster requires at least 3 healthy nodes"

[[constraint]]
name = "deploy_safe"
check = "cluster.healthy_nodes >= 3 AND cluster.cpu_avg < 80"
severity = "error"
message = "Cannot deploy: cluster not healthy enough"

[[workload]]
name = "pares-agens"
image = "nixos#pares-radix"
replicas = 1
placement = "prefer_idle"
gates = ["deploy_safe"]

[workload.resources]
cpu = 2.0
memory = "4G"

[workload.health]
protocol = "http"
target = "http://localhost:18789/health"
interval_secs = 30

[[workload]]
name = "pluresdb-peer"
image = "nixos#pluresdb"
replicas = "all"
placement = "everywhere"

[workload.resources]
cpu = 1.0
memory = "2G"

[workload.health]
protocol = "tcp"
target = "tcp://localhost:3100"
interval_secs = 10

[[rule]]
name = "self_heal"
when_condition = "workload.health_failures >= 3"
action = "restart workload on different_node"
log_message = "Self-healed {workload.name}"

[[rule]]
name = "auto_scale"
when_condition = "workload.cpu_avg > 70 for 5m"
action = "scale workload.replicas + 1"
max = "cluster.node_count"
log_message = "Auto-scaled {workload.name} to {workload.replicas}"
"#;

    #[test]
    fn parse_sample_px() {
        let px = parse(SAMPLE).expect("should parse");
        assert_eq!(px.node_requirements().len(), 1);
        assert_eq!(px.constraints().len(), 1);
        assert_eq!(px.workloads().len(), 2);
        assert_eq!(px.rules().len(), 2);

        let w0 = &px.workload[0];
        assert_eq!(w0.name, "pares-agens");
        assert_eq!(w0.placement, PlacementStrategy::PreferIdle);
        assert_eq!(w0.gates, vec!["deploy_safe"]);

        let w1 = &px.workload[1];
        assert!(w1.replicas.is_all());
        assert_eq!(w1.placement, PlacementStrategy::Everywhere);
    }

    #[test]
    fn health_check_parsing() {
        let px = parse(SAMPLE).unwrap();
        let h = px.workload[0].health.as_ref().unwrap();
        assert_eq!(h.protocol, "http");
        assert_eq!(h.interval_secs, 30);
    }

    #[test]
    fn roundtrip_serialize() {
        let px = parse(SAMPLE).unwrap();
        let json = serde_json::to_string(&px).unwrap();
        let back: PxFile = serde_json::from_str(&json).unwrap();
        assert_eq!(back.workload.len(), 2);
    }

    #[test]
    fn parse_discovery_config() {
        let input = r#"
[discovery]

[[discovery.direct]]
address = "praxisbot.local"
port = 7700

[[discovery.direct]]
address = "10.0.0.5"
port = 7700

[discovery.lan]
enabled = true
multicast_group = "239.255.77.77"
port = 7700

[discovery.hyperswarm]
enabled = false
topic_key = ""
"#;
        let px = parse(input).unwrap();
        let disc = px.discovery.unwrap();
        assert_eq!(disc.direct.len(), 2);
        assert_eq!(disc.direct[0].address, "praxisbot.local");
        assert_eq!(disc.direct[0].port, 7700);
        let lan = disc.lan.unwrap();
        assert!(lan.enabled);
        assert_eq!(lan.multicast_group, "239.255.77.77");
        let hs = disc.hyperswarm.unwrap();
        assert!(!hs.enabled);
    }
}
