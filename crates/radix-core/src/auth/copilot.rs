//! GitHub Copilot device flow authentication and model client.

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use tokio::sync::Mutex;
use tokio::sync::RwLock;
use tokio::time::sleep;

use crate::model::{
    ChatMessage, ChatOptions, ModelClient, ModelCompletion, ToolCall, ToolDefinition,
};

const COPILOT_CLIENT_ID: &str = "Iv1.b507a08c87ecfe98";
const DEVICE_CODE_URL: &str = "https://github.com/login/device/code";
const OAUTH_TOKEN_URL: &str = "https://github.com/login/oauth/access_token";
const COPILOT_TOKEN_URL: &str = "https://api.github.com/copilot_internal/v2/token";
const DEFAULT_API_BASE: &str = "https://api.individual.githubcopilot.com";
const EDITOR_VERSION: &str = "vscode/1.96.2";
const USER_AGENT: &str = "GitHubCopilotChat/0.26.7";
const API_VERSION: &str = "2025-04-01";
const INTEGRATION_ID: &str = "vscode-chat";
const COPILOT_CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
#[cfg(not(test))]
const COPILOT_READ_TIMEOUT: Duration = Duration::from_secs(120);
#[cfg(test)]
const COPILOT_READ_TIMEOUT: Duration = Duration::from_millis(200);

fn copilot_http_client() -> Result<reqwest::Client, reqwest::Error> {
    reqwest::Client::builder()
        .http1_only()
        .pool_max_idle_per_host(0)
        // Fail fast if the endpoint is unreachable, and abort a stream that
        // goes silent (server accepted the connection but stops sending SSE
        // data — a real failure mode that otherwise hangs the whole agentic turn
        // forever / eternal Telegram hourglass). read_timeout is per-read, so
        // it does NOT cap legitimately long streams — only a stalled one with no
        // bytes for the interval.
        .connect_timeout(COPILOT_CONNECT_TIMEOUT)
        .read_timeout(COPILOT_READ_TIMEOUT)
        .build()
}

fn copilot_http_client_or_panic() -> reqwest::Client {
    copilot_http_client().expect("failed to build HTTP client")
}

/// Errors emitted during Copilot authentication or token refresh.
#[derive(Debug, Error)]
pub enum CopilotAuthError {
    /// HTTP request failed.
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    /// JSON serialization/deserialization failed.
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    /// Response was missing required fields.
    #[error("invalid response: {0}")]
    InvalidResponse(String),
    /// OAuth endpoint returned an error.
    #[error("oauth error: {0}")]
    OAuth(String),
}

/// Tracks OAuth and Copilot session tokens.
#[derive(Debug, Clone)]
pub struct CopilotAuth {
    #[allow(dead_code)]
    client_id: String,
    oauth_token: Option<String>,
    session_token: Option<String>,
    session_expires_at: u64,
    api_base_url: String,
    #[allow(dead_code)]
    client: reqwest::Client,
}

impl CopilotAuth {
    /// Create a new Copilot auth state using an existing OAuth token.
    pub fn new(oauth_token: String) -> Self {
        Self {
            client_id: COPILOT_CLIENT_ID.to_string(),
            oauth_token: Some(oauth_token),
            session_token: None,
            session_expires_at: 0,
            api_base_url: DEFAULT_API_BASE.to_string(),
            client: copilot_http_client_or_panic(),
        }
    }

    /// Start the Copilot device flow.
    pub async fn device_flow_start() -> Result<(String, String, String), CopilotAuthError> {
        #[derive(Deserialize)]
        struct DeviceCodeResponse {
            device_code: String,
            user_code: String,
            verification_uri: String,
        }

        let client = copilot_http_client_or_panic();
        let response = client
            .post(DEVICE_CODE_URL)
            .header(ACCEPT, "application/json")
            .form(&[("client_id", COPILOT_CLIENT_ID), ("scope", "copilot")])
            .send()
            .await?
            .error_for_status()?;

        let payload: DeviceCodeResponse = response.json().await?;
        Ok((
            payload.device_code,
            payload.user_code,
            payload.verification_uri,
        ))
    }

    /// Poll the device flow until an OAuth token is issued.
    pub async fn device_flow_poll(device_code: &str) -> Result<String, CopilotAuthError> {
        #[derive(Deserialize)]
        struct TokenResponse {
            access_token: Option<String>,
            error: Option<String>,
            error_description: Option<String>,
        }

        let client = copilot_http_client_or_panic();
        let mut interval = Duration::from_secs(5);
        loop {
            let response = client
                .post(OAUTH_TOKEN_URL)
                .header(ACCEPT, "application/json")
                .form(&[
                    ("client_id", COPILOT_CLIENT_ID),
                    ("device_code", device_code),
                    ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ])
                .send()
                .await?
                .error_for_status()?;

            let payload: TokenResponse = response.json().await?;
            if let Some(token) = payload.access_token {
                return Ok(token);
            }

            if let Some(error) = payload.error {
                match error.as_str() {
                    "authorization_pending" => {
                        sleep(interval).await;
                        continue;
                    }
                    "slow_down" => {
                        interval += Duration::from_secs(5);
                        sleep(interval).await;
                        continue;
                    }
                    _ => {
                        let detail = payload
                            .error_description
                            .unwrap_or_else(|| "unknown error".into());
                        return Err(CopilotAuthError::OAuth(format!("{error}: {detail}")));
                    }
                }
            }

            return Err(CopilotAuthError::InvalidResponse(
                "missing access_token".into(),
            ));
        }
    }

    /// Exchange the OAuth token for a Copilot session token.
    pub async fn exchange_copilot_token(
        oauth_token: &str,
    ) -> Result<(String, u64, String), CopilotAuthError> {
        #[derive(Deserialize)]
        struct CopilotTokenResponse {
            token: String,
            expires_at: Value,
        }

        let client = copilot_http_client_or_panic();
        tracing::info!(
            url = COPILOT_TOKEN_URL,
            "exchanging OAuth token for Copilot session token"
        );
        let response = client
            .get(COPILOT_TOKEN_URL)
            .header(AUTHORIZATION, format!("Bearer {oauth_token}"))
            .header("Editor-Version", EDITOR_VERSION)
            .header("User-Agent", USER_AGENT)
            .header("X-Github-Api-Version", API_VERSION)
            .send()
            .await?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            tracing::error!(%status, body = &body[..body.len().min(500)], "Copilot token exchange failed");
            return Err(CopilotAuthError::OAuth(format!(
                "token exchange failed ({status}): {}",
                &body[..body.len().min(200)]
            )));
        }
        let payload: CopilotTokenResponse = response.json().await?;
        let expires_at = match payload.expires_at {
            Value::Number(num) => num
                .as_u64()
                .ok_or_else(|| CopilotAuthError::InvalidResponse("invalid expires_at".into()))?,
            Value::String(s) => s
                .parse::<u64>()
                .map_err(|_| CopilotAuthError::InvalidResponse("invalid expires_at".into()))?,
            _ => {
                return Err(CopilotAuthError::InvalidResponse(
                    "invalid expires_at".into(),
                ))
            }
        };

        let api_base =
            extract_api_base_url(&payload.token).unwrap_or_else(|| DEFAULT_API_BASE.to_string());

        tracing::info!(
            api_base = %api_base,
            expires_at = expires_at,
            "Copilot session token acquired"
        );

        Ok((payload.token, expires_at, api_base))
    }

    /// Ensure the session token is fresh; refresh if needed.
    pub async fn ensure_fresh_token(&mut self) -> Result<&str, CopilotAuthError> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| CopilotAuthError::InvalidResponse("time error".into()))?
            .as_secs();

        let needs_refresh = match self.session_token {
            Some(_) => now + 60 >= self.session_expires_at,
            None => true,
        };

        if needs_refresh {
            tracing::info!("Copilot session token expired or missing, refreshing");
            let oauth_token = self
                .oauth_token
                .clone()
                .ok_or_else(|| CopilotAuthError::InvalidResponse("missing oauth token".into()))?;
            let (session_token, expires_at, api_base) =
                Self::exchange_copilot_token(&oauth_token).await?;
            self.session_token = Some(session_token);
            self.session_expires_at = expires_at;
            self.api_base_url = api_base;
        }

        self.session_token
            .as_deref()
            .ok_or_else(|| CopilotAuthError::InvalidResponse("missing session token".into()))
    }

    /// Current API base URL derived from the session token.
    pub fn api_base_url(&self) -> &str {
        &self.api_base_url
    }
}

/// Model client that talks directly to GitHub Copilot.
#[derive(Clone)]
pub struct CopilotModelClient {
    auth: Arc<Mutex<CopilotAuth>>,
    model: Arc<RwLock<String>>,
    client: reqwest::Client,
    /// Ordered list of fallback models to try when the primary returns a 4xx error.
    fallback_models: Vec<String>,
}

impl CopilotModelClient {
    /// Create a Copilot model client for the given model.
    pub fn new(auth: CopilotAuth, model: impl Into<String>) -> Self {
        Self::new_with_model_handle(auth, Arc::new(RwLock::new(model.into())))
    }

    /// Create a Copilot model client backed by a shared model handle.
    pub fn new_with_model_handle(auth: CopilotAuth, model: Arc<RwLock<String>>) -> Self {
        Self {
            auth: Arc::new(Mutex::new(auth)),
            model,
            client: copilot_http_client_or_panic(),
            fallback_models: vec![],
        }
    }

    /// Set fallback models to try when the primary model fails with a 4xx error.
    pub fn with_fallbacks(mut self, models: Vec<String>) -> Self {
        self.fallback_models = models;
        self
    }
}

#[async_trait]
impl ModelClient for CopilotModelClient {
    async fn complete(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolDefinition],
        options: &ChatOptions,
    ) -> Result<ModelCompletion, String> {
        let (token, api_base) = {
            let mut auth = self.auth.lock().await;
            let token = auth.ensure_fresh_token().await.map_err(|e| e.to_string())?;
            (token.to_string(), auth.api_base_url().to_string())
        };

        let mut rendered_messages: Vec<Value> = Vec::with_capacity(messages.len());
        for message in messages {
            let mut obj = serde_json::Map::new();
            obj.insert("role".into(), Value::String(message.role.clone()));
            obj.insert("content".into(), Value::String(message.content.clone()));
            if let Some(tool_call_id) = &message.tool_call_id {
                obj.insert("tool_call_id".into(), Value::String(tool_call_id.clone()));
            }
            if let Some(tool_calls) = &message.tool_calls {
                let calls: Vec<Value> = tool_calls
                    .iter()
                    .map(|call| {
                        serde_json::json!({
                            "id": call.id,
                            "type": "function",
                            "function": {
                                "name": call.name,
                                "arguments": call.arguments.to_string(),
                            }
                        })
                    })
                    .collect();
                obj.insert("tool_calls".into(), Value::Array(calls));
            }
            rendered_messages.push(Value::Object(obj));
        }

        let model = self.model.read().await.clone();
        let mut body = serde_json::json!({
            "model": model,
            "messages": rendered_messages,
        });

        if !tools.is_empty() {
            let tool_defs: Vec<Value> = tools
                .iter()
                .map(|tool| {
                    serde_json::json!({
                        "type": "function",
                        "function": {
                            "name": tool.name,
                            "description": tool.description,
                            "parameters": tool.parameters,
                        }
                    })
                })
                .collect();
            body["tools"] = Value::Array(tool_defs);
        }

        if let Some(temp) = options.temperature {
            body["temperature"] = Value::Number(
                serde_json::Number::from_f64(temp).unwrap_or_else(|| serde_json::Number::from(0)),
            );
        }
        if options.logprobs {
            body["logprobs"] = Value::Bool(true);
        }

        let url = format!("{}/chat/completions", api_base.trim_end_matches('/'));
        let request_id = uuid::Uuid::new_v4();
        let request_start = std::time::Instant::now();
        tracing::info!(
            url = %url,
            model = %model,
            %request_id,
            message_count = messages.len(),
            tool_count = tools.len(),
            "sending Copilot completion request"
        );
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {token}")).map_err(|e| e.to_string())?,
        );
        headers.insert("Editor-Version", HeaderValue::from_static(EDITOR_VERSION));
        headers.insert("User-Agent", HeaderValue::from_static(USER_AGENT));
        headers.insert(
            "X-Github-Api-Version",
            HeaderValue::from_static(API_VERSION),
        );
        headers.insert(
            "Copilot-Integration-Id",
            HeaderValue::from_static(INTEGRATION_ID),
        );
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        let response = self
            .client
            .post(&url)
            .headers(headers.clone())
            .json(&body)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        let status = response.status();
        let body_text = response
            .text()
            .await
            .map_err(|e| format!("response body read error: {e}"))?;

        // Retry logic for transient failures.
        let (status, body_text) = if status == reqwest::StatusCode::MISDIRECTED_REQUEST {
            // 421: HTTP/2 connection coalescing issue — rebuild client and retry once.
            tracing::warn!(
                attempt = 1,
                "421 Misdirected Request — retrying with fresh connection"
            );
            let fresh_client = copilot_http_client()
                .map_err(|e| format!("failed to build fresh HTTP client: {e}"))?;
            let resp = fresh_client
                .post(&url)
                .headers(headers.clone())
                .json(&body)
                .send()
                .await
                .map_err(|e| e.to_string())?;
            let s = resp.status();
            let t = resp
                .text()
                .await
                .map_err(|e| format!("response body read error: {e}"))?;
            (s, t)
        } else if status == reqwest::StatusCode::UNAUTHORIZED {
            // 401: token may have expired — refresh and retry once.
            tracing::warn!(
                attempt = 1,
                "401 Unauthorized — refreshing token and retrying"
            );
            let new_token = {
                let mut auth = self.auth.lock().await;
                auth.ensure_fresh_token()
                    .await
                    .map_err(|e| e.to_string())?
                    .to_string()
            };
            let mut retry_headers = headers.clone();
            retry_headers.insert(
                AUTHORIZATION,
                HeaderValue::from_str(&format!("Bearer {new_token}")).map_err(|e| e.to_string())?,
            );
            let resp = self
                .client
                .post(&url)
                .headers(retry_headers)
                .json(&body)
                .send()
                .await
                .map_err(|e| e.to_string())?;
            let s = resp.status();
            let t = resp
                .text()
                .await
                .map_err(|e| format!("response body read error: {e}"))?;
            (s, t)
        } else if status.is_server_error() {
            // 5xx: retry up to 2 times with exponential backoff.
            let mut last_status = status;
            let mut last_body = body_text;
            let backoffs = [Duration::from_secs(1), Duration::from_secs(3)];
            for (i, delay) in backoffs.iter().enumerate() {
                tracing::warn!(attempt = i + 1, status = %last_status, delay_ms = delay.as_millis(), "server error — retrying");
                sleep(*delay).await;
                let resp = self
                    .client
                    .post(&url)
                    .headers(headers.clone())
                    .json(&body)
                    .send()
                    .await
                    .map_err(|e| e.to_string())?;
                last_status = resp.status();
                last_body = resp
                    .text()
                    .await
                    .map_err(|e| format!("response body read error: {e}"))?;
                if !last_status.is_server_error() {
                    break;
                }
            }
            (last_status, last_body)
        } else {
            (status, body_text)
        };
        tracing::info!(
            %status,
            %request_id,
            latency_ms = request_start.elapsed().as_millis(),
            body_len = body_text.len(),
            body_preview = &body_text[..body_text.len().min(200)],
            "Copilot completion response"
        );

        let payload: Value = serde_json::from_str(&body_text).map_err(|e| {
            format!(
                "error decoding response body: {e}\nBody: {}",
                &body_text[..body_text.len().min(500)]
            )
        })?;
        if !status.is_success() {
            let is_client_error = status.is_client_error();
            let err_msg = format!("copilot error ({status}): {payload}");

            // On 4xx errors, try fallback models before giving up.
            // Only attempt fallbacks from the primary call (check if current
            // model matches the configured primary to avoid recursion).
            let primary_model = {
                // Re-read in case it was swapped — but since we hold no
                // lock across the HTTP call this is fine.
                self.model.read().await.clone()
            };
            if is_client_error && !self.fallback_models.is_empty() && model == primary_model {
                tracing::warn!(
                    model = %model,
                    status = %status,
                    "primary model failed with client error, trying fallbacks"
                );
                for fallback in &self.fallback_models {
                    tracing::info!(model = %fallback, "trying fallback model");
                    *self.model.write().await = fallback.clone();
                    let result = self.complete(messages, tools, options).await;
                    *self.model.write().await = primary_model.clone();
                    match result {
                        Ok(completion) => {
                            tracing::info!(model = %fallback, "fallback model succeeded");
                            return Ok(completion);
                        }
                        Err(e) => {
                            tracing::warn!(model = %fallback, error = %e, "fallback model also failed");
                        }
                    }
                }
            }

            return Err(err_msg);
        }

        let choice = payload
            .get("choices")
            .and_then(|v| v.as_array())
            .and_then(|arr| arr.first())
            .ok_or_else(|| "model returned no choices".to_string())?;

        let message = choice
            .get("message")
            .ok_or_else(|| "model returned no message".to_string())?;

        let content = message
            .get("content")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let tool_calls = message
            .get("tool_calls")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|call| {
                        let id = call.get("id")?.as_str()?.to_string();
                        let function = call.get("function")?;
                        let name = function.get("name")?.as_str()?.to_string();
                        let args_raw = function.get("arguments");
                        let args_value = args_raw
                            .and_then(|v| v.as_str())
                            .and_then(|s| serde_json::from_str::<Value>(s).ok())
                            .unwrap_or_else(|| args_raw.cloned().unwrap_or(Value::Null));
                        Some(ToolCall {
                            id,
                            name,
                            arguments: args_value,
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        // Fix #604: OpenAI API returns logprobs.content[{token, logprob, ...}],
        // not logprobs.token_logprobs.
        let logprobs = choice
            .get("logprobs")
            .and_then(|v| v.get("content"))
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.get("logprob").and_then(|lp| lp.as_f64()))
                    .collect::<Vec<f64>>()
            });

        Ok(ModelCompletion {
            content,
            tool_calls,
            logprobs,
            model: None,
        })
    }

    fn context_window(&self) -> Option<u64> {
        // Estimate context window from model name.
        // Use try_read to avoid blocking in async contexts.
        let model = self.model.try_read().ok()?;
        Some(estimate_context_window(&model))
    }

    fn model_id(&self) -> Option<String> {
        self.model.try_read().ok().map(|m| m.clone())
    }
}

// ---------------------------------------------------------------------------
// Model Discovery
// ---------------------------------------------------------------------------

/// Information about a model available through the Copilot API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AvailableModel {
    /// Model identifier (e.g. "claude-sonnet-4.5", "gpt-4o").
    pub id: String,
    /// Human-readable name.
    #[serde(default)]
    pub name: String,
    /// Maximum input tokens.
    #[serde(default)]
    pub max_input_tokens: Option<u64>,
    /// Maximum output tokens.
    #[serde(default)]
    pub max_output_tokens: Option<u64>,
    /// Supported capabilities (e.g. ["chat", "tools"]).
    #[serde(default)]
    pub capabilities: Vec<String>,
}

impl AvailableModel {
    /// Context window for this model.
    ///
    /// Prefers the real discovered `max_input_tokens` from the provider; falls
    /// back to a name-based estimate only when the provider did not report it.
    /// Returns `None` only if there is genuinely no basis for an estimate
    /// (never a fabricated placeholder).
    pub fn context_window(&self) -> Option<u64> {
        self.max_input_tokens
            .or_else(|| Some(estimate_context_window(&self.id)))
    }

    /// Tier classification derived from the model id.
    pub fn tier(&self) -> ModelTier {
        classify_model_tier(&self.id)
    }
}

/// Model tier classification for smart selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ModelTier {
    /// Fast, cheap models for simple tasks.
    Fast,
    /// Balanced models for most work.
    Standard,
    /// High-capability models for complex reasoning.
    Premium,
}

/// Result of model discovery — recommended selections with fallback chains.
#[derive(Debug, Clone)]
pub struct ModelSelection {
    /// Primary model for standard inference.
    pub primary: String,
    /// Deep/escalation model for complex reasoning.
    pub deep: String,
    /// Fast model for simple/short responses.
    pub fast: Option<String>,
    /// Ordered fallback chains per tier (best → worst within tier, then cross-tier).
    pub fallbacks: ModelFallbacks,
    /// All available models.
    pub available: Vec<AvailableModel>,
}

/// Ordered fallback chains for graceful model degradation.
/// Each chain is sorted by preference (best first). When the primary choice
/// for a tier fails, iterate the chain to find the next usable model.
/// If the entire tier is exhausted, fall through to the next lower tier's chain.
#[derive(Debug, Clone, Default)]
pub struct ModelFallbacks {
    /// Premium tier fallback chain (for deep model failures).
    pub premium: Vec<String>,
    /// Standard tier fallback chain (for primary model failures).
    pub standard: Vec<String>,
    /// Fast tier fallback chain (for fast model failures).
    pub fast: Vec<String>,
}

impl CopilotAuth {
    /// List models available through the Copilot API.
    ///
    /// Tries `/models` on the Copilot API base URL first, falls back to the
    /// GitHub Models catalog at `https://models.github.ai/catalog/models`.
    pub async fn list_models(&mut self) -> Result<Vec<AvailableModel>, CopilotAuthError> {
        let token = self.ensure_fresh_token().await?.to_string();
        let api_base = self.api_base_url().to_string();
        let client = copilot_http_client_or_panic();

        // Try Copilot API /models endpoint
        let url = format!("{}/models", api_base.trim_end_matches('/'));
        tracing::info!(url = %url, "listing available models");

        let response = client
            .get(&url)
            .header(AUTHORIZATION, format!("Bearer {token}"))
            .header("Editor-Version", EDITOR_VERSION)
            .header("User-Agent", USER_AGENT)
            .header("X-Github-Api-Version", API_VERSION)
            .header(ACCEPT, "application/json")
            .send()
            .await?;

        if response.status().is_success() {
            let body: Value = response.json().await?;
            if let Some(models) = parse_models_response(&body) {
                tracing::info!(count = models.len(), "discovered models from Copilot API");
                return Ok(models);
            }
        } else {
            tracing::debug!(
                status = %response.status(),
                "Copilot /models endpoint unavailable, trying catalog"
            );
        }

        // Fallback: GitHub Models catalog
        let catalog_url = "https://models.github.ai/catalog/models";
        let response = client
            .get(catalog_url)
            .header(AUTHORIZATION, format!("Bearer {token}"))
            .header(ACCEPT, "application/json")
            .send()
            .await?;

        if response.status().is_success() {
            let body: Value = response.json().await?;
            if let Some(models) = parse_models_response(&body) {
                tracing::info!(
                    count = models.len(),
                    source = "catalog",
                    "discovered models from GitHub catalog"
                );
                return Ok(models);
            }
        }

        tracing::warn!("no model listing available from any endpoint");
        Ok(vec![])
    }
}

/// Parse a models response — handles both array-of-objects and {data: [...]} formats.
fn parse_models_response(body: &Value) -> Option<Vec<AvailableModel>> {
    let arr = if let Some(arr) = body.as_array() {
        arr.clone()
    } else if let Some(arr) = body.get("data").and_then(|d| d.as_array()) {
        arr.clone()
    } else if let Some(arr) = body.get("models").and_then(|d| d.as_array()) {
        arr.clone()
    } else {
        return None;
    };

    let models: Vec<AvailableModel> = arr
        .iter()
        .filter_map(|v| {
            let id = v.get("id").and_then(|i| i.as_str())?.to_string();
            let name = v
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or(&id)
                .to_string();
            let max_input_tokens = v
                .get("limits")
                .and_then(|l| l.get("max_input_tokens"))
                .and_then(|t| t.as_u64())
                .or_else(|| v.get("max_input_tokens").and_then(|t| t.as_u64()));
            let max_output_tokens = v
                .get("limits")
                .and_then(|l| l.get("max_output_tokens"))
                .and_then(|t| t.as_u64())
                .or_else(|| v.get("max_output_tokens").and_then(|t| t.as_u64()));
            let capabilities = v
                .get("capabilities")
                .and_then(|c| c.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|c| c.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();

            Some(AvailableModel {
                id,
                name,
                max_input_tokens,
                max_output_tokens,
                capabilities,
            })
        })
        .collect();

    if models.is_empty() {
        None
    } else {
        Some(models)
    }
}

/// Estimate a model's context window from its name.
/// Returns conservative estimates; discovery-based `max_input_tokens` overrides these.
pub fn estimate_context_window(model_id: &str) -> u64 {
    let id = model_id.to_lowercase();

    // Premium models — typically largest context windows
    if id.contains("opus") || id.contains("o1") || id.contains("o3") {
        return 200_000;
    }
    if id.contains("gpt-5") {
        return 256_000;
    }
    // Standard models
    if id.contains("sonnet") || id.contains("claude") {
        return 200_000;
    }
    if id.contains("gpt-4o") && !id.contains("mini") {
        return 128_000;
    }
    if id.contains("gpt-4.1") || id.contains("gpt-4.5") {
        return 128_000;
    }
    if id.contains("gemini") {
        return 1_000_000; // Gemini has massive context
    }
    // Fast models — usually smaller context
    if id.contains("haiku") {
        return 200_000;
    }
    if id.contains("mini") {
        return 128_000;
    }
    if id.contains("flash") {
        return 1_000_000; // Gemini Flash still has large context
    }
    if id.contains("nano") {
        return 32_000;
    }
    // Conservative default
    128_000
}

/// Classify a model into a tier based on its identifier.
pub fn classify_model_tier(model_id: &str) -> ModelTier {
    let id = model_id.to_lowercase();

    // Premium tier — large reasoning models
    if id.contains("opus")
        || id.contains("o1")
        || id.contains("o3")
        || id.contains("gpt-5")
        || id.contains("deep-research")
    {
        return ModelTier::Premium;
    }

    // Standard tier — balanced capability models
    if id.contains("sonnet")
        || id.contains("gpt-4o")
        || id.contains("gpt-4.1")
        || id.contains("gemini-2")
        || id.contains("claude-4")
    {
        return ModelTier::Standard;
    }

    // Fast tier — smaller/cheaper models
    if id.contains("haiku")
        || id.contains("mini")
        || id.contains("flash")
        || id.contains("nano")
        || id.contains("gpt-3")
    {
        return ModelTier::Fast;
    }

    // Default to standard for unknown models
    ModelTier::Standard
}

/// Score a model within its tier for selection preference.
/// Higher = better. Based on recency and capability.
fn model_preference_score(model_id: &str) -> u32 {
    let id = model_id.to_lowercase();

    // Claude models — prefer newer versions
    if id.contains("claude-opus-4") {
        return 100;
    }
    if id.contains("claude-sonnet-4") {
        return 95;
    }
    if id.contains("claude-4") {
        return 90;
    }
    if id.contains("claude-opus") {
        return 85;
    }
    if id.contains("claude-sonnet") {
        return 80;
    }

    // GPT models
    if id.contains("gpt-5") {
        return 98;
    }
    if id.contains("gpt-4o") {
        return 75;
    }
    if id.contains("gpt-4.1") {
        return 70;
    }
    if id.contains("o3") {
        return 92;
    }
    if id.contains("o1") {
        return 88;
    }

    // Gemini
    if id.contains("gemini-2.5") {
        return 72;
    }
    if id.contains("gemini-2") {
        return 68;
    }

    50 // Unknown
}

/// Select the best primary, deep, and fast models from a list of available models.
/// Builds ordered fallback chains for graceful degradation when a model becomes unavailable.
pub fn select_models(available: &[AvailableModel]) -> ModelSelection {
    if available.is_empty() {
        tracing::warn!("no models discovered, using hardcoded defaults");
        return ModelSelection {
            primary: "claude-sonnet-4.5".to_string(),
            deep: "claude-opus-4.6".to_string(),
            fast: None,
            fallbacks: ModelFallbacks::default(),
            available: vec![],
        };
    }

    // Separate into tiers
    let mut premium: Vec<&AvailableModel> = vec![];
    let mut standard: Vec<&AvailableModel> = vec![];
    let mut fast: Vec<&AvailableModel> = vec![];

    for m in available {
        match classify_model_tier(&m.id) {
            ModelTier::Premium => premium.push(m),
            ModelTier::Standard => standard.push(m),
            ModelTier::Fast => fast.push(m),
        }
    }

    // Sort each tier by preference (highest score first)
    premium.sort_by_key(|m| std::cmp::Reverse(model_preference_score(&m.id)));
    standard.sort_by_key(|m| std::cmp::Reverse(model_preference_score(&m.id)));
    fast.sort_by_key(|m| std::cmp::Reverse(model_preference_score(&m.id)));

    // Build fallback chains (all models in tier, ordered by preference)
    let premium_chain: Vec<String> = premium.iter().map(|m| m.id.clone()).collect();
    let standard_chain: Vec<String> = standard.iter().map(|m| m.id.clone()).collect();
    let fast_chain: Vec<String> = fast.iter().map(|m| m.id.clone()).collect();

    // Primary: best Standard-tier model (balanced speed/quality)
    let primary = standard
        .first()
        .map(|m| m.id.clone())
        .or_else(|| fast.first().map(|m| m.id.clone()))
        .unwrap_or_else(|| available[0].id.clone());

    // Deep: best Premium-tier model (max reasoning for escalation)
    let deep = premium
        .first()
        .map(|m| m.id.clone())
        .or_else(|| {
            // Fallback to best Standard model that's different from primary
            standard
                .iter()
                .find(|m| m.id != primary)
                .map(|m| m.id.clone())
        })
        .unwrap_or_else(|| primary.clone());

    // Fast: best Fast-tier model (cheapest/quickest)
    let fast_pick = fast.first().map(|m| m.id.clone());

    tracing::info!(
        primary = %primary,
        deep = %deep,
        fast = ?fast_pick,
        total_available = available.len(),
        premium_count = premium.len(),
        standard_count = standard.len(),
        fast_count = fast.len(),
        "model selection complete"
    );

    ModelSelection {
        primary,
        deep,
        fast: fast_pick,
        fallbacks: ModelFallbacks {
            premium: premium_chain,
            standard: standard_chain,
            fast: fast_chain,
        },
        available: available.to_vec(),
    }
}

fn extract_api_base_url(token: &str) -> Option<String> {
    // Method 1: Extract from proxy-ep field in semicolon-delimited token
    // (how OpenClaw does it — the token contains routing metadata)
    if let Some(proxy_ep) = token
        .split(';')
        .find_map(|part| part.trim().strip_prefix("proxy-ep="))
    {
        let host = proxy_ep.trim();
        if !host.is_empty() {
            // proxy.business.githubcopilot.com → api.business.githubcopilot.com
            let api_host = if host.starts_with("proxy.") {
                host.replacen("proxy.", "api.", 1)
            } else {
                host.to_string()
            };
            let base = if api_host.starts_with("https://") || api_host.starts_with("http://") {
                api_host
            } else {
                format!("https://{api_host}")
            };
            tracing::info!(proxy_ep = host, api_base = %base, "derived API base from token proxy-ep");
            return Some(base);
        }
    }

    // Method 2: Extract from JWT vscu claim (fallback)
    let mut parts = token.split('.');
    parts.next()?;
    let payload = parts.next()?;
    let decoded = URL_SAFE_NO_PAD.decode(payload.as_bytes()).ok()?;
    let payload_json: Value = serde_json::from_slice(&decoded).ok()?;
    payload_json
        .get("vscu")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};

    fn seeded_auth(api_base_url: String) -> CopilotAuth {
        CopilotAuth {
            client_id: COPILOT_CLIENT_ID.to_string(),
            oauth_token: Some("oauth-token".to_string()),
            session_token: Some("session-token".to_string()),
            session_expires_at: u64::MAX,
            api_base_url,
            client: copilot_http_client_or_panic(),
        }
    }

    async fn read_request(stream: &mut TcpStream) {
        let mut buf = Vec::new();
        let mut chunk = [0_u8; 1024];
        let header_end = loop {
            let n = stream.read(&mut chunk).await.expect("read request");
            if n == 0 {
                return;
            }
            buf.extend_from_slice(&chunk[..n]);
            if let Some(pos) = buf.windows(4).position(|window| window == b"\r\n\r\n") {
                break pos + 4;
            }
        };

        let headers = String::from_utf8_lossy(&buf[..header_end]);
        let content_length = headers
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().ok())
                    .flatten()
            })
            .unwrap_or(0);

        while buf.len() < header_end + content_length {
            let n = stream.read(&mut chunk).await.expect("read body");
            if n == 0 {
                break;
            }
            buf.extend_from_slice(&chunk[..n]);
        }
    }

    async fn write_json_response(stream: &mut TcpStream, status: &str, body: &str) {
        let response = format!(
            "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        );
        stream
            .write_all(response.as_bytes())
            .await
            .expect("write response");
    }

    async fn spawn_421_then_hang_server() -> (String, Arc<AtomicUsize>) {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind test server");
        let addr = listener.local_addr().expect("test server addr");
        let attempts = Arc::new(AtomicUsize::new(0));
        let server_attempts = Arc::clone(&attempts);

        tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                let attempt = server_attempts.fetch_add(1, Ordering::SeqCst) + 1;
                tokio::spawn(async move {
                    read_request(&mut stream).await;
                    if attempt == 1 {
                        write_json_response(
                            &mut stream,
                            "421 Misdirected Request",
                            r#"{"error":"misdirected"}"#,
                        )
                        .await;
                    } else {
                        tokio::time::sleep(Duration::from_secs(5)).await;
                    }
                });

                if attempt >= 2 {
                    break;
                }
            }
        });

        (format!("http://{addr}"), attempts)
    }

    #[tokio::test]
    async fn copilot_421_retry_is_bounded_by_retry_client_read_timeout() {
        let (api_base, attempts) = spawn_421_then_hang_server().await;
        let client = CopilotModelClient::new(seeded_auth(api_base), "test-model");

        let result = tokio::time::timeout(
            Duration::from_secs(2),
            client.complete(&[ChatMessage::user("hello")], &[], &ChatOptions::default()),
        )
        .await;

        assert!(
            result.is_ok(),
            "421 retry should be bounded by the shared Copilot HTTP read_timeout"
        );
        let err = result
            .expect("outer timeout checked")
            .expect_err("hanging retry response should time out");
        let err_lower = err.to_ascii_lowercase();
        assert!(
            err_lower.contains("timed out")
                || err_lower.contains("operation timed out")
                || err_lower.contains("error sending request"),
            "unexpected retry error: {err}"
        );
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
    }
}
