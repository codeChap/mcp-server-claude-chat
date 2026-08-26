use moka::future::Cache;
use reqwest::Method;
use rmcp::{
    ErrorData as McpError, ServerHandler, handler::server::tool::ToolRouter,
    handler::server::wrapper::Parameters, model::*, tool, tool_handler, tool_router,
};
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;
use tracing::debug;

use crate::api::{
    AnthropicClient, Message, MessagesRequest, MessagesResponse, ModelsResponse, Thinking,
};
use crate::params::{ChatParams, SearchParams, VisionParams};

const DEFAULT_MODEL: &str = "claude-opus-4-8";
const DEFAULT_MAX_TOKENS: u32 = 4096;
const MIN_THINKING_BUDGET: u32 = 1024;

/// Web search server-tool identifier. The stable version without dynamic
/// filtering (the newer `web_search_20260209` additionally requires the code
/// execution tool). Update if Anthropic releases a newer stable version.
const WEB_SEARCH_TOOL_TYPE: &str = "web_search_20250305";

/// Roles Anthropic accepts inside the `messages` array. System instructions go
/// in the top-level `system` field, not as a message.
const VALID_ROLES: &[&str] = &["user", "assistant"];

/// The MCP server wrapping the Anthropic Claude Messages API.
#[derive(Clone)]
pub struct ClaudeServer {
    client: Arc<AnthropicClient>,
    default_model: String,
    default_max_tokens: u32,
    models_cache: Cache<(), String>,
    tool_router: ToolRouter<Self>,
}

// ---------------------------------------------------------------------------
// Shared helpers — keep tool methods DRY
// ---------------------------------------------------------------------------

impl ClaudeServer {
    /// Validate temperature is finite and within Anthropic's 0.0–1.0 range.
    fn validate_temperature(temp: Option<f32>) -> Result<(), McpError> {
        if let Some(t) = temp
            && (!t.is_finite() || !(0.0..=1.0).contains(&t))
        {
            return Err(McpError::invalid_params(
                format!("temperature must be a finite number between 0.0 and 1.0, got {t}"),
                None,
            ));
        }
        Ok(())
    }

    /// Build the messages vec from optional history JSON plus the current prompt.
    /// Validates that history roles are `user`/`assistant` only.
    fn build_messages(history_json: Option<&str>, prompt: &str) -> Result<Vec<Message>, String> {
        let mut messages = Vec::new();

        if let Some(json) = history_json {
            let parsed: Vec<Message> =
                serde_json::from_str(json).map_err(|e| format!("Invalid messages JSON: {e}"))?;
            for m in &parsed {
                if !VALID_ROLES.contains(&m.role.as_str()) {
                    return Err(format!(
                        "Invalid role '{}' in messages — Anthropic accepts only: {}. \
                         Use the system_prompt parameter for system instructions.",
                        m.role,
                        VALID_ROLES.join(", ")
                    ));
                }
            }
            messages.extend(parsed);
        }

        messages.push(Message::user(prompt));
        Ok(messages)
    }

    /// Resolve the model to use for a call, falling back to the server default.
    fn resolve_model(&self, model: Option<&str>) -> String {
        model
            .map(str::to_string)
            .unwrap_or_else(|| self.default_model.clone())
    }

    /// Send a Messages request and return the formatted result.
    async fn do_messages(&self, req: &MessagesRequest) -> Result<CallToolResult, McpError> {
        match self
            .client
            .request::<_, MessagesResponse>(Method::POST, "/messages", Some(req))
            .await
        {
            Ok(resp) => Ok(CallToolResult::success(vec![Content::text(
                resp.to_string(),
            )])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }
}

// ---------------------------------------------------------------------------
// Tool definitions
// ---------------------------------------------------------------------------

#[tool_router]
impl ClaudeServer {
    pub fn new(
        client: AnthropicClient,
        default_model: Option<String>,
        default_max_tokens: Option<u32>,
    ) -> Self {
        let models_cache = Cache::builder()
            .max_capacity(1)
            .time_to_live(Duration::from_secs(300))
            .build();

        Self {
            client: Arc::new(client),
            default_model: default_model.unwrap_or_else(|| DEFAULT_MODEL.to_string()),
            default_max_tokens: default_max_tokens.unwrap_or(DEFAULT_MAX_TOKENS),
            models_cache,
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        description = "Send a chat message to Claude (Anthropic Messages API). Supports multi-turn \
                       history, a system prompt, model selection, and extended thinking via \
                       thinking_budget."
    )]
    async fn chat(
        &self,
        Parameters(p): Parameters<ChatParams>,
    ) -> Result<CallToolResult, McpError> {
        debug!(model = ?p.model, "chat tool called");
        Self::validate_temperature(p.temperature)?;

        let thinking = match p.thinking_budget {
            Some(b) if b < MIN_THINKING_BUDGET => {
                return Err(McpError::invalid_params(
                    format!("thinking_budget must be >= {MIN_THINKING_BUDGET}, got {b}"),
                    None,
                ));
            }
            Some(b) => Some(Thinking::enabled(b)),
            None => None,
        };

        // max_tokens must exceed the thinking budget (it counts toward the cap),
        // so leave room for the actual answer on top of the budget.
        let mut max_tokens = p.max_tokens.unwrap_or(self.default_max_tokens);
        if let Some(b) = p.thinking_budget
            && max_tokens <= b
        {
            max_tokens = b + self.default_max_tokens;
        }

        let messages = Self::build_messages(p.messages.as_deref(), &p.prompt)
            .map_err(|e| McpError::invalid_params(e, None))?;

        let req = MessagesRequest {
            model: self.resolve_model(p.model.as_deref()),
            max_tokens,
            messages,
            system: p.system_prompt,
            // Extended thinking is incompatible with a custom temperature.
            temperature: if thinking.is_some() {
                None
            } else {
                p.temperature
            },
            tools: None,
            thinking,
            stop_sequences: None,
        };

        self.do_messages(&req).await
    }

    #[tool(
        description = "Analyse an image with Claude's vision capability. Provide an image URL \
                       (http:// or https://) and a text prompt."
    )]
    async fn chat_with_vision(
        &self,
        Parameters(p): Parameters<VisionParams>,
    ) -> Result<CallToolResult, McpError> {
        debug!(model = ?p.model, "chat_with_vision tool called");
        if !p.image_url.starts_with("http://") && !p.image_url.starts_with("https://") {
            return Err(McpError::invalid_params(
                "image_url must start with http:// or https://",
                None,
            ));
        }
        Self::validate_temperature(p.temperature)?;

        let req = MessagesRequest {
            model: self.resolve_model(p.model.as_deref()),
            max_tokens: p.max_tokens.unwrap_or(self.default_max_tokens),
            messages: vec![Message::user_with_image(&p.prompt, &p.image_url)],
            system: None,
            temperature: p.temperature,
            tools: None,
            thinking: None,
            stop_sequences: None,
        };

        self.do_messages(&req).await
    }

    #[tool(
        description = "Chat with Claude using live web search. Claude decides when to search, runs \
                       queries server-side, and returns an answer with cited sources."
    )]
    async fn chat_with_search(
        &self,
        Parameters(p): Parameters<SearchParams>,
    ) -> Result<CallToolResult, McpError> {
        debug!(model = ?p.model, "chat_with_search tool called");
        Self::validate_temperature(p.temperature)?;

        let mut tool = serde_json::json!({
            "type": WEB_SEARCH_TOOL_TYPE,
            "name": "web_search",
        });
        if let Some(mu) = p.max_uses {
            tool["max_uses"] = Value::from(mu);
        }

        let req = MessagesRequest {
            model: self.resolve_model(p.model.as_deref()),
            max_tokens: p.max_tokens.unwrap_or(self.default_max_tokens),
            messages: vec![Message::user(&p.prompt)],
            system: p.system_prompt,
            temperature: p.temperature,
            tools: Some(vec![tool]),
            thinking: None,
            stop_sequences: None,
        };

        self.do_messages(&req).await
    }

    #[tool(description = "List available Claude models and their IDs (cached for 5 minutes).")]
    async fn list_models(&self) -> Result<CallToolResult, McpError> {
        if let Some(cached) = self.models_cache.get(&()).await {
            debug!("list_models: returning cached result");
            return Ok(CallToolResult::success(vec![Content::text(cached.clone())]));
        }

        debug!("list_models: fetching from API");
        match self
            .client
            .request::<(), ModelsResponse>(Method::GET, "/models?limit=1000", None)
            .await
        {
            Ok(resp) => {
                let lines: Vec<String> = resp
                    .data
                    .iter()
                    .map(|m| match &m.display_name {
                        Some(name) => format!("- {} ({})", m.id, name),
                        None => format!("- {}", m.id),
                    })
                    .collect();
                let result = if lines.is_empty() {
                    "No models returned.".to_string()
                } else {
                    lines.join("\n")
                };
                self.models_cache.insert((), result.clone()).await;
                Ok(CallToolResult::success(vec![Content::text(result)]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }
}

// ---------------------------------------------------------------------------
// MCP ServerHandler
// ---------------------------------------------------------------------------

#[tool_handler]
impl ServerHandler for ClaudeServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new(
                "claude-chat",
                env!("CARGO_PKG_VERSION"),
            ))
            .with_instructions(
                "Anthropic Claude MCP server. Tools: chat (with optional extended thinking), \
                 chat_with_vision, chat_with_search (live web search), list_models.",
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- validate_temperature -------------------------------------------------

    #[test]
    fn validate_temperature_none_is_ok() {
        assert!(ClaudeServer::validate_temperature(None).is_ok());
    }

    #[test]
    fn validate_temperature_valid_range() {
        assert!(ClaudeServer::validate_temperature(Some(0.0)).is_ok());
        assert!(ClaudeServer::validate_temperature(Some(0.5)).is_ok());
        assert!(ClaudeServer::validate_temperature(Some(1.0)).is_ok());
    }

    #[test]
    fn validate_temperature_out_of_range() {
        assert!(ClaudeServer::validate_temperature(Some(-0.1)).is_err());
        // 2.0 is valid for OpenAI-style APIs but NOT for Anthropic.
        assert!(ClaudeServer::validate_temperature(Some(1.1)).is_err());
        assert!(ClaudeServer::validate_temperature(Some(2.0)).is_err());
    }

    #[test]
    fn validate_temperature_non_finite() {
        assert!(ClaudeServer::validate_temperature(Some(f32::NAN)).is_err());
        assert!(ClaudeServer::validate_temperature(Some(f32::INFINITY)).is_err());
    }

    // -- build_messages -------------------------------------------------------

    #[test]
    fn build_messages_basic() {
        let msgs = ClaudeServer::build_messages(None, "hello").unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].role, "user");
    }

    #[test]
    fn build_messages_with_history() {
        let history =
            r#"[{"role": "user", "content": "hi"}, {"role": "assistant", "content": "hey"}]"#;
        let msgs = ClaudeServer::build_messages(Some(history), "next").unwrap();
        assert_eq!(msgs.len(), 3);
        assert_eq!(msgs[0].role, "user");
        assert_eq!(msgs[1].role, "assistant");
        assert_eq!(msgs[2].role, "user");
    }

    #[test]
    fn build_messages_rejects_system_role() {
        let history = r#"[{"role": "system", "content": "be nice"}]"#;
        let err = ClaudeServer::build_messages(Some(history), "hi").unwrap_err();
        assert!(err.contains("Invalid role 'system'"));
    }

    #[test]
    fn build_messages_invalid_json() {
        let result = ClaudeServer::build_messages(Some("not json"), "hello");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid messages JSON"));
    }
}
