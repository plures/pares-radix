//! MCP server — stdio-based JSON-RPC 2.0 server implementing the MCP protocol.
//!
//! Reads requests from stdin (newline-delimited JSON), dispatches them, and
//! writes responses to stdout. Supports server-initiated notifications via an
//! optional event receiver (e.g., sub-agent completion events).

use std::sync::Arc;

use serde_json::{json, Value};
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

use pares_radix_mcp_client::protocol::{JsonRpcError, JsonRpcRequest, JsonRpcResponse};

use crate::handler::ToolHandler;

/// A server-initiated notification to push to the MCP client.
///
/// These are JSON-RPC messages with no `id` — the client must not respond.
#[derive(Debug, Clone)]
pub struct ServerNotification {
    /// The notification method (e.g., `notifications/message`).
    pub method: String,
    /// Optional params payload.
    pub params: Option<Value>,
}

impl ServerNotification {
    /// Create a notification for a sub-agent completion.
    pub fn subagent_completed(
        session_id: &str,
        agent_name: &str,
        result: Result<&str, &str>,
        duration_secs: f64,
        undelivered_steerings: &[String],
    ) -> Self {
        let (status, output) = match result {
            Ok(output) => ("completed", output),
            Err(error) => ("failed", error),
        };
        Self {
            method: "notifications/message".to_string(),
            params: Some(json!({
                "level": if status == "completed" { "info" } else { "error" },
                "logger": "subagent",
                "data": {
                    "event": "subagent_completed",
                    "session_id": session_id,
                    "agent_name": agent_name,
                    "status": status,
                    "output": output,
                    "duration_secs": duration_secs,
                    "undelivered_steerings": undelivered_steerings
                }
            })),
        }
    }
}

/// MCP server that communicates over stdio.
pub struct McpServer {
    handler: Arc<dyn ToolHandler>,
    server_name: String,
    server_version: String,
    /// Optional receiver for server-initiated notifications (e.g., sub-agent completions).
    notification_rx: Option<mpsc::UnboundedReceiver<ServerNotification>>,
}

impl McpServer {
    /// Create a new MCP server with the given tool handler.
    pub fn new(handler: Arc<dyn ToolHandler>) -> Self {
        Self {
            handler,
            server_name: "pares-radix".to_string(),
            server_version: env!("CARGO_PKG_VERSION").to_string(),
            notification_rx: None,
        }
    }

    /// Override the server name reported in `initialize`.
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.server_name = name.into();
        self
    }

    /// Attach a notification receiver for server-initiated messages.
    ///
    /// When the server is running, it will `select!` between stdin and this
    /// receiver, pushing notifications to stdout as they arrive.
    pub fn with_notifications(mut self, rx: mpsc::UnboundedReceiver<ServerNotification>) -> Self {
        self.notification_rx = Some(rx);
        self
    }

    /// Create a notification sender/receiver pair.
    ///
    /// Returns the sender (caller keeps it to push notifications) and consumes
    /// self to produce a configured server.
    pub fn with_notification_channel(self) -> (mpsc::UnboundedSender<ServerNotification>, Self) {
        let (tx, rx) = mpsc::unbounded_channel();
        (tx, self.with_notifications(rx))
    }

    /// Run the MCP server, reading from stdin and writing to stdout.
    ///
    /// This function blocks until stdin is closed (EOF). If a notification
    /// receiver is attached, the server will also push server-initiated
    /// notifications to stdout as they arrive.
    pub async fn run(mut self) -> Result<(), McpServerError> {
        let stdin = io::stdin();
        let mut stdout = io::stdout();
        let reader = BufReader::new(stdin);
        let mut lines = reader.lines();

        info!(server = %self.server_name, "MCP server starting on stdio");

        loop {
            // Select between stdin input and server-initiated notifications.
            let action = if let Some(ref mut rx) = self.notification_rx {
                tokio::select! {
                    line = lines.next_line() => StdioAction::Line(line),
                    notif = rx.recv() => StdioAction::Notification(notif),
                }
            } else {
                StdioAction::Line(lines.next_line().await)
            };

            match action {
                StdioAction::Line(result) => {
                    let line = match result.map_err(McpServerError::Io)? {
                        Some(l) => l,
                        None => break, // EOF
                    };
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
                            stdout
                                .write_all(b"\n")
                                .await
                                .map_err(McpServerError::Io)?;
                            stdout.flush().await.map_err(McpServerError::Io)?;
                            continue;
                        }
                    };

                    // Notifications (no id) don't get responses
                    let request_id = match request.id {
                        Some(id) => id,
                        None => {
                            debug!(method = %request.method, "notification received (no response)");
                            if request.method == "notifications/initialized" {
                                info!("client sent initialized notification");
                            }
                            continue;
                        }
                    };

                    let response =
                        self.handle_request(&request.method, request.params).await;

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

                    let response_json =
                        serde_json::to_string(&json_response).unwrap_or_default();
                    debug!(response = %response_json, "sending response");
                    stdout
                        .write_all(response_json.as_bytes())
                        .await
                        .map_err(McpServerError::Io)?;
                    stdout
                        .write_all(b"\n")
                        .await
                        .map_err(McpServerError::Io)?;
                    stdout.flush().await.map_err(McpServerError::Io)?;
                }
                StdioAction::Notification(notif) => {
                    let Some(notif) = notif else {
                        // Sender dropped — notifications are done, keep serving
                        self.notification_rx = None;
                        debug!("notification sender dropped, continuing without notifications");
                        continue;
                    };
                    Self::write_notification(&mut stdout, &notif).await?;
                }
            }
        }

        info!("stdin closed, MCP server shutting down");
        Ok(())
    }

    /// Write a server-initiated notification (no `id`) to the output stream.
    async fn write_notification(
        stdout: &mut io::Stdout,
        notification: &ServerNotification,
    ) -> Result<(), McpServerError> {
        let msg = json!({
            "jsonrpc": "2.0",
            "method": notification.method,
            "params": notification.params
        });
        let json_str = serde_json::to_string(&msg).unwrap_or_default();
        debug!(notification = %json_str, "sending server notification");
        stdout
            .write_all(json_str.as_bytes())
            .await
            .map_err(McpServerError::Io)?;
        stdout.write_all(b"\n").await.map_err(McpServerError::Io)?;
        stdout.flush().await.map_err(McpServerError::Io)?;
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

        let arguments = params.get("arguments").cloned().unwrap_or(json!({}));

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

/// Internal action type for the `select!` loop in `McpServer::run`.
enum StdioAction {
    /// A line was read from stdin (or EOF/error).
    Line(Result<Option<String>, std::io::Error>),
    /// A server-initiated notification arrived.
    Notification(Option<ServerNotification>),
}

/// Errors from the MCP server.
#[derive(Debug, thiserror::Error)]
pub enum McpServerError {
    /// IO error reading/writing stdio.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Spawn a background task that bridges `CompletionEvent`s from the delegation
/// manager into `ServerNotification`s on the given sender.
///
/// Returns a `JoinHandle` for the forwarding task. The task exits when the
/// completion receiver is closed (all sub-agent managers dropped).
pub fn spawn_completion_forwarder(
    mut completion_rx: mpsc::UnboundedReceiver<pares_agens_core::delegation::CompletionEvent>,
    notification_tx: mpsc::UnboundedSender<ServerNotification>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        while let Some(event) = completion_rx.recv().await {
            let result = match &event.result {
                Ok(output) => Ok(output.as_str()),
                Err(error) => Err(error.as_str()),
            };
            let notif = ServerNotification::subagent_completed(
                &event.session_id.to_string(),
                &event.agent_name,
                result,
                event.duration.as_secs_f64(),
                &event.undelivered_steerings,
            );
            if notification_tx.send(notif).is_err() {
                // Receiver dropped (server shut down) — stop forwarding
                break;
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handler::ToolResult;

    struct MockHandler;

    #[async_trait::async_trait]
    impl ToolHandler for MockHandler {
        async fn list_tools(&self) -> Vec<pares_radix_mcp_client::protocol::Tool> {
            vec![pares_radix_mcp_client::protocol::Tool {
                name: "test_tool".to_string(),
                description: Some("A test tool".to_string()),
                input_schema: pares_radix_mcp_client::protocol::ToolInputSchema {
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
        assert!(content[0]["text"].as_str().unwrap().contains("test_tool"));
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

    #[test]
    fn server_notification_subagent_completed_success() {
        let notif = ServerNotification::subagent_completed(
            "sess-123",
            "code-reviewer",
            Ok("LGTM, no issues found."),
            45.2,
            &[],
        );
        assert_eq!(notif.method, "notifications/message");
        let params = notif.params.unwrap();
        assert_eq!(params["level"], "info");
        assert_eq!(params["logger"], "subagent");
        assert_eq!(params["data"]["event"], "subagent_completed");
        assert_eq!(params["data"]["session_id"], "sess-123");
        assert_eq!(params["data"]["agent_name"], "code-reviewer");
        assert_eq!(params["data"]["status"], "completed");
        assert_eq!(params["data"]["output"], "LGTM, no issues found.");
        assert_eq!(params["data"]["duration_secs"], 45.2);
        assert!(params["data"]["undelivered_steerings"].as_array().unwrap().is_empty());
    }

    #[test]
    fn server_notification_subagent_completed_failure() {
        let notif = ServerNotification::subagent_completed(
            "sess-456",
            "builder",
            Err("cargo build failed: missing dependency"),
            12.8,
            &["hurry up".to_string(), "use --release".to_string()],
        );
        let params = notif.params.unwrap();
        assert_eq!(params["level"], "error");
        assert_eq!(params["data"]["status"], "failed");
        assert_eq!(params["data"]["output"], "cargo build failed: missing dependency");
        let steerings = params["data"]["undelivered_steerings"].as_array().unwrap();
        assert_eq!(steerings.len(), 2);
        assert_eq!(steerings[0], "hurry up");
    }

    #[test]
    fn with_notification_channel_creates_pair() {
        let handler: Arc<dyn ToolHandler> = Arc::new(MockHandler);
        let server = McpServer::new(handler);
        let (tx, server) = server.with_notification_channel();
        assert!(server.notification_rx.is_some());
        // Sender should be usable
        tx.send(ServerNotification {
            method: "test".to_string(),
            params: None,
        })
        .unwrap();
    }
}
