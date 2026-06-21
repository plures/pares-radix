//! Hook system — plugin-facing lifecycle intercept points.
//!
//! Hooks let plugins intercept and modify the agent's processing pipeline
//! at well-defined points: before/after tool execution, before/after model
//! calls, on inbound messages, and on errors.
//!
//! The [`HookManager`] collects registrations from plugins and fires them
//! sequentially at each hook point.  A hook can [`Continue`], [`Block`] the
//! action, [`ModifyContext`], or [`InjectContext`] into the prompt.

use std::sync::RwLock;

use serde::{Deserialize, Serialize};

/// Points in the agent lifecycle where hooks can intercept.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HookPoint {
    /// Before a tool executes.
    PreToolUse,
    /// After a tool executes.
    PostToolUse,
    /// Before sending messages to the model.
    PreModelCall,
    /// After the model responds.
    PostModelCall,
    /// When a user message arrives.
    OnMessage,
    /// When any error occurs.
    OnError,
}

impl HookPoint {
    /// Parse a hook point from a string (case-insensitive).
    pub fn from_str_loose(s: &str) -> Option<Self> {
        match s.to_lowercase().replace(['-', '_'], "").as_str() {
            "pretooluse" => Some(Self::PreToolUse),
            "posttooluse" => Some(Self::PostToolUse),
            "premodelcall" => Some(Self::PreModelCall),
            "postmodelcall" => Some(Self::PostModelCall),
            "onmessage" => Some(Self::OnMessage),
            "onerror" => Some(Self::OnError),
            _ => None,
        }
    }
}

/// The action a hook requests after inspection.
#[derive(Debug, Clone)]
pub enum HookAction {
    /// Proceed normally.
    Continue,
    /// Modify the context before continuing.
    ModifyContext(serde_json::Value),
    /// Block the action with a reason.
    Block(String),
    /// Inject additional text into the prompt.
    InjectContext(String),
}

/// Context passed to hook callbacks.
#[derive(Debug, Clone, Default)]
pub struct HookContext {
    pub tool_name: Option<String>,
    pub tool_args: Option<serde_json::Value>,
    pub tool_result: Option<String>,
    pub model_prompt: Option<String>,
    pub model_response: Option<String>,
    pub message_text: Option<String>,
    pub error: Option<String>,
}

/// A registered hook from a plugin.
pub struct HookRegistration {
    pub plugin_name: String,
    pub hook_point: HookPoint,
    pub description: String,
    pub callback: Box<dyn Fn(&HookContext) -> HookAction + Send + Sync>,
}

/// Manages all registered hooks and fires them at the appropriate points.
pub struct HookManager {
    hooks: RwLock<Vec<HookRegistration>>,
}

impl HookManager {
    /// Create a new empty hook manager.
    pub fn new() -> Self {
        Self {
            hooks: RwLock::new(Vec::new()),
        }
    }

    /// Register a hook.
    pub fn register(&self, hook: HookRegistration) {
        if let Ok(mut hooks) = self.hooks.write() {
            hooks.push(hook);
        }
    }

    /// Unregister all hooks for a given plugin.
    pub fn unregister_plugin(&self, plugin_name: &str) {
        if let Ok(mut hooks) = self.hooks.write() {
            hooks.retain(|h| h.plugin_name != plugin_name);
        }
    }

    /// Fire all hooks registered for `point`, returning the composite action.
    ///
    /// Hooks are fired in registration order.  The first `Block` or
    /// `ModifyContext` wins.  Multiple `InjectContext` results are concatenated.
    /// If all hooks return `Continue`, the result is `Continue`.
    pub fn fire(&self, point: HookPoint, context: &mut HookContext) -> HookAction {
        let hooks = match self.hooks.read() {
            Ok(h) => h,
            Err(_) => return HookAction::Continue,
        };

        let mut injected = Vec::new();

        for hook in hooks.iter().filter(|h| h.hook_point == point) {
            match (hook.callback)(context) {
                HookAction::Continue => {}
                HookAction::Block(reason) => return HookAction::Block(reason),
                HookAction::ModifyContext(value) => return HookAction::ModifyContext(value),
                HookAction::InjectContext(text) => injected.push(text),
            }
        }

        if injected.is_empty() {
            HookAction::Continue
        } else {
            HookAction::InjectContext(injected.join("\n"))
        }
    }

    /// List all registered hooks (plugin name, point, description).
    pub fn list(&self) -> Vec<(String, HookPoint, String)> {
        match self.hooks.read() {
            Ok(hooks) => hooks
                .iter()
                .map(|h| (h.plugin_name.clone(), h.hook_point, h.description.clone()))
                .collect(),
            Err(_) => Vec::new(),
        }
    }
}

impl Default for HookManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Hook declaration in a plugin manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookDeclaration {
    pub point: String,
    pub description: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hook_point_from_str_loose() {
        assert_eq!(
            HookPoint::from_str_loose("PreToolUse"),
            Some(HookPoint::PreToolUse)
        );
        assert_eq!(
            HookPoint::from_str_loose("pre-tool-use"),
            Some(HookPoint::PreToolUse)
        );
        assert_eq!(
            HookPoint::from_str_loose("post_model_call"),
            Some(HookPoint::PostModelCall)
        );
        assert_eq!(HookPoint::from_str_loose("invalid"), None);
    }

    #[test]
    fn fire_returns_continue_when_no_hooks() {
        let mgr = HookManager::new();
        let mut ctx = HookContext::default();
        assert!(matches!(
            mgr.fire(HookPoint::PreToolUse, &mut ctx),
            HookAction::Continue
        ));
    }

    #[test]
    fn fire_returns_block_when_hook_blocks() {
        let mgr = HookManager::new();
        mgr.register(HookRegistration {
            plugin_name: "audit".into(),
            hook_point: HookPoint::PreToolUse,
            description: "block dangerous tools".into(),
            callback: Box::new(|ctx| {
                if ctx.tool_name.as_deref() == Some("rm") {
                    HookAction::Block("dangerous tool blocked".into())
                } else {
                    HookAction::Continue
                }
            }),
        });

        let mut ctx = HookContext {
            tool_name: Some("rm".into()),
            ..Default::default()
        };
        assert!(matches!(
            mgr.fire(HookPoint::PreToolUse, &mut ctx),
            HookAction::Block(_)
        ));

        let mut ctx2 = HookContext {
            tool_name: Some("read".into()),
            ..Default::default()
        };
        assert!(matches!(
            mgr.fire(HookPoint::PreToolUse, &mut ctx2),
            HookAction::Continue
        ));
    }

    #[test]
    fn fire_concatenates_inject_context() {
        let mgr = HookManager::new();
        mgr.register(HookRegistration {
            plugin_name: "a".into(),
            hook_point: HookPoint::OnMessage,
            description: "inject a".into(),
            callback: Box::new(|_| HookAction::InjectContext("context-a".into())),
        });
        mgr.register(HookRegistration {
            plugin_name: "b".into(),
            hook_point: HookPoint::OnMessage,
            description: "inject b".into(),
            callback: Box::new(|_| HookAction::InjectContext("context-b".into())),
        });

        let mut ctx = HookContext::default();
        match mgr.fire(HookPoint::OnMessage, &mut ctx) {
            HookAction::InjectContext(text) => {
                assert!(text.contains("context-a"));
                assert!(text.contains("context-b"));
            }
            other => panic!("expected InjectContext, got: {other:?}"),
        }
    }

    #[test]
    fn unregister_plugin_removes_all_hooks() {
        let mgr = HookManager::new();
        mgr.register(HookRegistration {
            plugin_name: "test".into(),
            hook_point: HookPoint::PreToolUse,
            description: "hook 1".into(),
            callback: Box::new(|_| HookAction::Continue),
        });
        mgr.register(HookRegistration {
            plugin_name: "test".into(),
            hook_point: HookPoint::PostToolUse,
            description: "hook 2".into(),
            callback: Box::new(|_| HookAction::Continue),
        });
        assert_eq!(mgr.list().len(), 2);

        mgr.unregister_plugin("test");
        assert_eq!(mgr.list().len(), 0);
    }
}
