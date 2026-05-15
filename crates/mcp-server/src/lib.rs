//! `mcp-server` — MCP (Model Context Protocol) server for pares-radix.
//!
//! Exposes pares-radix tools as an MCP server over stdio transport.
//! External agents (e.g., OpenClaw) can connect to this server and invoke
//! tools like `run_command`, `read_file`, `memory_search`, etc.
//!
//! # Architecture
//!
//! The server reads JSON-RPC 2.0 messages from stdin and writes responses to
//! stdout. It implements the MCP server protocol:
//!
//! 1. `initialize` — returns server capabilities (tools)
//! 2. `tools/list` — returns available tool definitions
//! 3. `tools/call` — executes a tool and returns results
//!
//! # Usage
//!
//! ```text
//! pares-radix mcp-serve
//! ```
//!
//! Or from another agent's MCP config:
//! ```json
//! {
//!   "command": "pares-radix",
//!   "args": ["mcp-serve"]
//! }
//! ```

#![warn(missing_docs)]

pub mod browser;
pub mod server;
pub mod handler;
pub mod radix_handler;

pub use server::McpServer;
pub use handler::ToolHandler;
pub use radix_handler::RadixToolHandler;
