//! MCP server — stdio-based JSON-RPC 2.0 server implementing the MCP protocol.
//!
//! Reads requests from stdin (newline-delimited JSON), dispatches them, and
//! writes responses to stdout.

use std::sync::Arc;

use serde_json::{json, Value};
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader};
use tracing::{debug, info, warn};

use mcp_client::protocol::{JsonRpcError, JsonRpcRequest, JsonRpcResponse};

use crate::handler::ToolHandler;

/// MCP server that communicates over stdio.
pub struct McpServer {
    handler: Arc<dyn ToolHandler>,
    server_name: String,
    server_version: String,
}

impl McpServer {
    /// Create a new MCP server with the given tool handler.
    pub fn new(handler: Arc<dyn ToolHandler>) -> Self {
        Self {
            handler,
            server_name: "pares-radix".to_string(),
            server_version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    /// Override the server name reported in `initialize`.
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.server_name = name.into();
        self
    }

    /// Run the MCP server, reading from stdin and writing to stdout.
    ///
    /// This function blocks until stdin is closed (EOF).
    pub async fn run(&self) -> Result<(), McpServerError> {
        let stdin = io::stdin();
        let mut stdout = io::stdout();
        let reader = BufReader::new(stdin);
        let mut lines = reader.lines();

        info!(server = %self.server_name, "MCP server starting on stdio");

        while let Some(line) = lines.next_line().await.map_err(McpServerError::Io)? {
            let line = line.trim().to_string();
            if line.is_empty() {
                continue;
            }

            debug!(raw = %line, "received message");

            let request: JsonRpcRequest = match serde_json::from_str(&line) {
                Ok(req) => req,
                Err(e) => {
                    warn!(error = %e, "invalid JSON-RPC message");
                    let error_response = JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id: Value::Null,
                        result: None,
                        error: Some(JsonRpcError {
                            code: -32700,
                            message: format!("Parse error: {e}"),
                            data: None,
                        }),
                    };
                    let response_json =
                        serde_json::to_string(&error_response).unwrap_or_default();
                    stdout
                        .write_all(response_json.as_bytes())
                        .await
                        .map_err(McpServerError::Io)?;
                    stdout.write_all(b"\n").await.map_err(McpServerError::Io)?;
                    stdout.flush().await.map_err(McpServerError::Io)?;
                    continue;
                }
            };

            // Notifications (no id) don't get responses
            let request_id = match request.id {
                Some(id) => id,
                None => {
                    debug!(method = %request.method, "notification received (no response)");
                    // Handle initialized notification
                    if request.method == "notifications/initialized" {
                        info!("client sent initialized notification");
                    }
                    continue;
                }
            };

            let response = self.handle_request(&request.method, request.params).await;

            let json_response = match response {
                Ok(result) => JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: request_id,
                    result: Some(result),
                    error: None,
                },
                Err(e) => JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: request_id,
                    result: None,
                    error: Some(e),
                },
            };

            let response_json = serde_json::to_string(&json_response).unwrap_or_default();
            debug!(response = %response_json, "sending response");
            stdout
                .write_all(response_json.as_bytes())
                .await
                .map_err(McpServerError::Io)?;
            stdout.write_all(b"\n").await.map_err(McpServerError::Io)?;
            stdout.flush().await.map_err(McpServerError::Io)?;
        }

        info!("stdin closed, MCP server shutting down");
        Ok(())
    }

    async fn handle_request(
        &self,
        method: &str,
        params: Option<Value>,
    ) -> Result<Value, JsonRpcError> {
        match method {
            "initialize" => self.handle_initialize().await,
            "tools/list" => self.handle_tools_list().await,
            "tools/call" => self.handle_tools_call(params).await,
            "ping" => Ok(json!({})),
            _ => Err(JsonRpcError {
                code: -32601,
                message: format!("Method not found: {method}"),
                data: None,
            }),
        }
    }

    async fn handle_initialize(&self) -> Result<Value, JsonRpcError> {
        let result = json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "tools": {
                    "listChanged": false
                }
            },
            "serverInfo": {
                "name": self.server_name,
                "version": self.server_version
            }
        });
        info!(
            server = %self.server_name,
            version = %self.server_version,
            "initialized"
        );
        Ok(result)
    }

    async fn handle_tools_list(&self) -> Result<Value, JsonRpcError> {
        let tools = self.handler.list_tools().await;
        let tools_json: Vec<Value> = tools
            .into_iter()
            .map(|t| {
                json!({
                    "name": t.name,
                    "description": t.description,
                    "inputSchema": t.input_schema
                })
            })
            .collect();

        Ok(json!({ "tools": tools_json }))
    }

    async fn handle_tools_call(&self, params: Option<Value>) -> Result<Value, JsonRpcError> {
        let params = params.ok_or_else(|| JsonRpcError {
            code: -32602,
            message: "Missing params for tools/call".to_string(),
            data: None,
        })?;

        let name = params
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| JsonRpcError {
                code: -32602,
                message: "Missing 'name' in tools/call params".to_string(),
                data: None,
            })?
            .to_string();

        let arguments = params
            .get("arguments")
            .cloned()
            .unwrap_or(json!({}));

        debug!(tool = %name, "calling tool");

        let result = self.handler.call_tool(&name, arguments).await;

        Ok(json!({
            "content": [{
                "type": "text",
                "text": result.content
            }],
            "isError": result.is_error
        }))
    }
}

/// Errors from the MCP server.
#[derive(Debug, thiserror::Error)]
pub enum McpServerError {
    /// IO error reading/writing stdio.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handler::ToolResult;

    struct MockHandler;

    #[async_trait::async_trait]
    impl ToolHandler for MockHandler {
        async fn list_tools(&self) -> Vec<mcp_client::protocol::Tool> {
            vec![mcp_client::protocol::Tool {
                name: "test_tool".to_string(),
                description: Some("A test tool".to_string()),
                input_schema: mcp_client::protocol::ToolInputSchema {
                    schema_type: "object".to_string(),
                    properties: Some(json!({"input": {"type": "string"}})),
                    required: Some(vec!["input".to_string()]),
                },
            }]
        }

        async fn call_tool(&self, name: &str, arguments: Value) -> ToolResult {
            ToolResult {
                content: format!("called {name} with {arguments}"),
                is_error: false,
            }
        }
    }

    #[tokio::test]
    async fn initialize_returns_server_info() {
        let handler: Arc<dyn ToolHandler> = Arc::new(MockHandler);
        let server = McpServer::new(handler);
        let result = server.handle_initialize().await.unwrap();
        assert_eq!(result["serverInfo"]["name"], "pares-radix");
        assert_eq!(result["protocolVersion"], "2024-11-05");
        assert!(result["capabilities"]["tools"].is_object());
    }

    #[tokio::test]
    async fn tools_list_returns_registered_tools() {
        let handler: Arc<dyn ToolHandler> = Arc::new(MockHandler);
        let server = McpServer::new(handler);
        let result = server.handle_tools_list().await.unwrap();
        let tools = result["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["name"], "test_tool");
        assert_eq!(tools[0]["description"], "A test tool");
    }

    #[tokio::test]
    async fn tools_call_dispatches_to_handler() {
        let handler: Arc<dyn ToolHandler> = Arc::new(MockHandler);
        let server = McpServer::new(handler);
        let params = Some(json!({
            "name": "test_tool",
            "arguments": {"input": "hello"}
        }));
        let result = server.handle_tools_call(params).await.unwrap();
        let content = result["content"].as_array().unwrap();
        assert_eq!(content[0]["type"], "text");
        assert!(content[0]["text"]
            .as_str()
            .unwrap()
            .contains("test_tool"));
        assert_eq!(result["isError"], false);
    }

    #[tokio::test]
    async fn tools_call_missing_name_returns_error() {
        let handler: Arc<dyn ToolHandler> = Arc::new(MockHandler);
        let server = McpServer::new(handler);
        let params = Some(json!({"arguments": {}}));
        let err = server.handle_tools_call(params).await.unwrap_err();
        assert_eq!(err.code, -32602);
    }

    #[tokio::test]
    async fn handle_request_unknown_method_returns_error() {
        let handler: Arc<dyn ToolHandler> = Arc::new(MockHandler);
        let server = McpServer::new(handler);
        let err = server
            .handle_request("nonexistent/method", None)
            .await
            .unwrap_err();
        assert_eq!(err.code, -32601);
    }

    #[tokio::test]
    async fn handle_request_ping_returns_empty_object() {
        let handler: Arc<dyn ToolHandler> = Arc::new(MockHandler);
        let server = McpServer::new(handler);
        let result = server.handle_request("ping", None).await.unwrap();
        assert_eq!(result, json!({}));
    }
}
