//! Cluster status formatting and query helpers.
//!
//! Shared by both the Telegram `/cluster` commands and the CLI `cluster`
//! subcommand so the output is consistent.

use crate::node::{ClusterNode, NodeStatus};
use crate::orchestrator::Orchestrator;
use crate::px_parser;
use crate::workload::WorkloadState;

/// Summary of current cluster state.
#[derive(Debug)]
pub struct ClusterSummary {
    pub nodes: Vec<ClusterNode>,
    pub total_cores: u32,
    pub total_memory_mb: u64,
    pub workload_count: usize,
}

impl ClusterSummary {
    pub fn from_nodes(nodes: &[ClusterNode]) -> Self {
        let online: Vec<_> = nodes.iter().filter(|n| n.status == NodeStatus::Online).collect();
        let total_cores: u32 = online.iter().map(|n| n.capabilities.cpu_cores).sum();
        let total_memory_mb: u64 = online.iter().map(|n| n.capabilities.memory_total_mb).sum();
        let workload_count: usize = online.iter().map(|n| n.workloads.len()).sum();
        ClusterSummary {
            nodes: nodes.to_vec(),
            total_cores,
            total_memory_mb,
            workload_count,
        }
    }
}

/// Format `/cluster status` output.
pub fn format_cluster_status(summary: &ClusterSummary) -> String {
    let online = summary
        .nodes
        .iter()
        .filter(|n| n.status == NodeStatus::Online)
        .count();
    let total_gb = summary.total_memory_mb / 1024;
    format!(
        "🖥️ Cluster: {} node{} | {} cores | {}GB RAM | {} workload{}",
        online,
        if online != 1 { "s" } else { "" },
        summary.total_cores,
        total_gb,
        summary.workload_count,
        if summary.workload_count != 1 { "s" } else { "" },
    )
}

/// Format `/cluster nodes` output.
pub fn format_cluster_nodes(nodes: &[ClusterNode]) -> String {
    if nodes.is_empty() {
        return "No nodes discovered.".to_string();
    }
    let summary = ClusterSummary::from_nodes(nodes);
    let mut out = format_cluster_status(&summary);

    for node in nodes {
        let status_str = match node.status {
            NodeStatus::Online if node.capabilities.is_idle => "online, idle",
            NodeStatus::Online => "online, active",
            NodeStatus::Offline => "offline",
            NodeStatus::Draining => "draining",
            NodeStatus::Maintenance => "maintenance",
        };
        out.push_str(&format!("\n\n{} ({})", node.hostname, status_str));
        let gpu_str = node
            .capabilities
            .gpu_model
            .as_deref()
            .unwrap_or(if node.capabilities.gpu { "yes" } else { "none" });
        out.push_str(&format!(
            "\n  CPU: {} cores | RAM: {}GB | GPU: {}",
            node.capabilities.cpu_cores,
            node.capabilities.memory_total_mb / 1024,
            gpu_str
        ));
        if node.workloads.is_empty() {
            out.push_str("\n  Workloads: none");
        } else {
            out.push_str(&format!("\n  Workloads: {}", node.workloads.join(", ")));
        }
    }
    out
}

/// Format `/cluster workloads` output.
pub fn format_cluster_workloads(workloads: &[WorkloadState]) -> String {
    if workloads.is_empty() {
        return "No active workloads.".to_string();
    }
    let mut out = format!("📦 {} workload{}:", workloads.len(), if workloads.len() != 1 { "s" } else { "" });
    for w in workloads {
        let status = format!("{:?}", w.status).to_lowercase();
        out.push_str(&format!(
            "\n• {} [{}] — nodes: {} | health failures: {}",
            w.name,
            status,
            if w.assigned_nodes.is_empty() {
                "unassigned".to_string()
            } else {
                w.assigned_nodes.join(", ")
            },
            w.health_failures,
        ));
    }
    out
}

/// Deploy a .px file: parse, evaluate constraints, and schedule.
pub fn format_deploy_result(px_content: &str, nodes: &[ClusterNode]) -> String {
    let config = match px_parser::parse(px_content) {
        Ok(c) => c,
        Err(e) => return format!("❌ Failed to parse .px file: {e}"),
    };

    let orch = Orchestrator::new(config, nodes.to_vec());
    let constraints = orch.evaluate_constraints();

    let mut out = String::new();
    let failed: Vec<_> = constraints.iter().filter(|c| !c.passed).collect();
    if !failed.is_empty() {
        out.push_str("⚠️ Constraint failures:\n");
        for c in &failed {
            out.push_str(&format!("  • {}: {}\n", c.constraint_name, c.message));
        }
    }

    let result = orch.reconcile();
    if !result.blocked.is_empty() {
        out.push_str("\n❌ Blocked workloads:\n");
        for b in &result.blocked {
            out.push_str(&format!("  • {}\n", b.message));
        }
    }
    if !result.assignments.is_empty() {
        out.push_str("\n✅ Scheduled:\n");
        for a in &result.assignments {
            out.push_str(&format!("  • {} → {}\n", a.workload_name, a.node_id));
        }
    }
    if out.is_empty() {
        "No workloads defined in .px file.".to_string()
    } else {
        out
    }
}

/// Format `/cluster info` (local node capabilities).
pub fn format_node_info(caps: &crate::node::NodeCapabilities) -> String {
    let gpu_str = if caps.gpu {
        caps.gpu_model
            .as_deref()
            .unwrap_or("detected")
            .to_string()
    } else {
        "none".to_string()
    };
    format!(
        "🔍 Local Node Info\n  OS: {} ({})\n  CPU: {} cores\n  RAM: {}GB\n  GPU: {}\n  Idle: {}\n  BitNet: {}",
        caps.os,
        caps.arch,
        caps.cpu_cores,
        caps.memory_total_mb / 1024,
        gpu_str,
        if caps.is_idle { "yes" } else { "no" },
        if caps.bitnet_available { "yes" } else { "no" },
    )
}
