//! Commitment detector — identifies promises in model responses and creates tasks.
//!
//! Listens for `DeliveryRequest` events (which contain the final model response text)
//! and scans for commitment language ("I'll", "I will", "Let me", etc.).
//! Detected commitments are stored as Tasks in the TaskManager.
//!
//! This is the pipeline equivalent of `agent.rs::detect_and_store_promises()`.
//! Decision logic should migrate to `commitment-detection.px` via PxBridge.

use std::sync::Arc;

use tracing::{debug, info};

use crate::spine::event::SpineEvent;
use crate::spine::pipeline::{PipelineEmitter, SpineProcedure};
use crate::task::{CompletionCondition, ConditionType};
use crate::task_manager::TaskManager;

/// Scans model responses for commitment language and creates tasks.
pub struct CommitmentDetector {
    task_manager: Arc<TaskManager>,
}

impl CommitmentDetector {
    pub fn new(task_manager: Arc<TaskManager>) -> Self {
        Self { task_manager }
    }
}

#[async_trait::async_trait]
impl SpineProcedure for CommitmentDetector {
    fn name(&self) -> &str {
        "commitment_detector"
    }

    fn handles(&self) -> Option<Vec<&'static str>> {
        Some(vec!["delivery_request"])
    }

    async fn handle(&self, event: &SpineEvent, _emitter: &PipelineEmitter) {
        let SpineEvent::DeliveryRequest {
            content, chat_id, ..
        } = event
        else {
            return;
        };

        let commitments = detect_commitments(content);
        if commitments.is_empty() {
            return;
        }

        info!(
            count = commitments.len(),
            chat_id = %chat_id,
            "commitment_detector: found {} commitment(s)",
            commitments.len()
        );

        for commitment in &commitments {
            self.task_manager.create_task(
                commitment,
                chat_id,
                vec![CompletionCondition {
                    description: format!("Complete: {}", &commitment[..commitment.len().min(80)]),
                    condition_type: ConditionType::ModelEvaluation(format!(
                        "Verify completion: {}",
                        &commitment[..commitment.len().min(120)]
                    )),
                    satisfied: false,
                }],
            );
            debug!(commitment = %commitment, "Created task from commitment");
        }
    }
}

/// Detect commitment language in model response text.
///
/// Mirrors the Rust fallback in `agent.rs::detect_and_store_promises()`.
/// TODO: Route through PxBridge.call("detect_commitments") once wired.
fn detect_commitments(text: &str) -> Vec<String> {
    let commitment_patterns = ["i'll ", "i will ", "let me ", "going to "];

    let action_verbs = [
        "diagnose", "fix", "implement", "write", "create", "update", "check", "verify", "build",
        "deploy", "configure", "refactor", "optimize", "debug", "test", "add", "remove", "migrate",
        "install", "resolve", "investigate", "wire", "connect", "integrate", "port", "rewrite",
    ];

    let mut promises: Vec<String> = Vec::new();

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.len() < 15 || trimmed.len() > 200 {
            continue;
        }

        // Numbered list items with action verbs
        if trimmed
            .chars()
            .next()
            .map_or(false, |c| c.is_ascii_digit())
        {
            if let Some(text_after) = trimmed.split_once('.').map(|(_, t)| t.trim()) {
                let lower = text_after.to_lowercase();
                if action_verbs.iter().any(|v| lower.starts_with(v)) {
                    promises.push(text_after.to_string());
                }
            }
        }

        // "I will..." / "I'll..." with action verbs
        let lower = trimmed.to_lowercase();
        for pattern in &commitment_patterns {
            if lower.contains(pattern) {
                if let Some(after) = lower.split_once(pattern).map(|(_, a)| a) {
                    if action_verbs.iter().any(|v| after.starts_with(v)) && after.len() >= 15 {
                        let dedup = !promises
                            .iter()
                            .any(|p| p.to_lowercase().contains(&after[..after.len().min(25)]));
                        if dedup {
                            promises.push(trimmed.to_string());
                        }
                    }
                }
            }
        }
    }

    // Cap at 5 commitments per response
    promises.truncate(5);
    promises
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_numbered_action_items() {
        let text = "Here's my plan:\n1. Fix the streaming issue\n2. Deploy to production\n3. Verify it works";
        let commitments = detect_commitments(text);
        assert_eq!(commitments.len(), 3);
        assert!(commitments[0].contains("Fix the streaming issue"));
    }

    #[test]
    fn detects_i_will_patterns() {
        let text = "I'll investigate the root cause and fix the pipeline connection.";
        let commitments = detect_commitments(text);
        assert_eq!(commitments.len(), 1);
    }

    #[test]
    fn ignores_short_lines() {
        let text = "I'll do it.\nOK.";
        let commitments = detect_commitments(text);
        assert!(commitments.is_empty());
    }

    #[test]
    fn caps_at_five() {
        let text = (1..=10)
            .map(|i| format!("{}. Fix module number {} in the codebase completely", i, i))
            .collect::<Vec<_>>()
            .join("\n");
        let commitments = detect_commitments(&text);
        assert_eq!(commitments.len(), 5);
    }
}
