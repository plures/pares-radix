//! Built-in tools — tools the agent has access to without external MCP.
//!
//! These are registered alongside MCP tools in the tool dispatcher.
//! Each module provides:
//!   - Tool definitions (JSON schema for the model)
//!   - A handler struct with `call(name, args)` method
//!   - A `handles_tool(name)` check for routing

pub mod task_registry;

pub use task_registry::TaskRegistryTool;
