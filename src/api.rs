use reqwest::{Client, Method};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;
use std::time::Duration;
use thiserror::Error;
use tracing::instrument;

/// API version header value sent on every request — Anthropic requires this.
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// Errors returned by the Anthropic API client.
#[derive(Error, Debug)]
pub enum ApiError {
    #[error("HTTP request failed: {0}")]
    Reqwest(#[from] reqwest::Error),

    #[error("Anthropic API error ({status}): {body}")]
    Api {
        status: reqwest::StatusCode,
        body: String,
    },
}

/// Shared HTTP client for all Anthropic API calls.
pub struct AnthropicClient {
    api_key: String,
    base_url: String,
    http: Client,
}

impl AnthropicClient {
    /// Create a new client pointing at the given base URL
    /// (typically `https://api.anthropic.com/v1`).
    pub fn new(api_key: String, base_url: String) -> Self {
        Self {
            api_key,
            base_url,
            http: Client::builder()
                .timeout(Duration::from_secs(300))
                .build()
                .expect("Failed to build reqwest client"),
        }
    }

    /// Unified HTTP request method — handles GET and POST with an optional body.
    /// Adds the `x-api-key` and `anthropic-version` headers Anthropic requires.
    #[instrument(skip(self, body), fields(path = %path))]
    pub async fn request<Req: Serialize, Resp: for<'de> Deserialize<'de>>(
        &self,
        method: Method,
        path: &str,
        body: Option<&Req>,
    ) -> Result<Resp, ApiError> {
        let url = format!("{}{path}", self.base_url);
        let mut builder = self
            .http
            .request(method, &url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION);

        if let Some(b) = body {
            builder = builder.json(b);
        }

        let response = builder.send().await?;

        let status = response.status();
        if !status.is_success() {
            let body = match response.text().await {
                Ok(text) => text,
                Err(e) => format!("<failed to read response body: {e}>"),
            };
            tracing::warn!(status = %status, "API request failed");
            return Err(ApiError::Api { status, body });
        }

        Ok(response.json::<Resp>().await?)
    }
}

// ---------------------------------------------------------------------------
// Messages API types
// ---------------------------------------------------------------------------

/// A request to the `/v1/messages` endpoint.
#[derive(Serialize)]
pub struct MessagesRequest {
    pub model: String,
    /// Required by the Anthropic API.
    pub max_tokens: u32,
    pub messages: Vec<Message>,
    /// Top-level system prompt (Anthropic keeps this out of the `messages` array).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<Thinking>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_sequences: Option<Vec<String>>,
}

/// A single message in a conversation. `content` is either a plain string or an
/// array of content blocks (text, image, …).
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Message {
    pub role: String,
    pub content: Value,
}

impl Message {
    /// Create a user message with plain text content.
    pub fn user(text: &str) -> Self {
        Self {
            role: "user".into(),
            content: Value::String(text.into()),
        }
    }

    /// Create a user message combining text and an image (referenced by URL).
    pub fn user_with_image(text: &str, image_url: &str) -> Self {
        Self {
            role: "user".into(),
            content: serde_json::json!([
                { "type": "text", "text": text },
                { "type": "image", "source": { "type": "url", "url": image_url } }
            ]),
        }
    }
}

/// Extended-thinking configuration. Serializes as `{"type": "enabled", "budget_tokens": N}`.
#[derive(Debug, Serialize)]
pub struct Thinking {
    #[serde(rename = "type")]
    pub kind: String,
    pub budget_tokens: u32,
}

impl Thinking {
    pub fn enabled(budget_tokens: u32) -> Self {
        Self {
            kind: "enabled".into(),
            budget_tokens,
        }
    }
}

/// The response from the `/v1/messages` endpoint. Content blocks are kept as raw
/// JSON values so new block types (tool use, search results, …) don't break parsing.
#[derive(Debug, Deserialize)]
pub struct MessagesResponse {
    #[serde(default)]
    pub content: Vec<Value>,
    #[serde(default)]
    pub stop_reason: Option<String>,
    #[serde(default)]
    pub usage: Option<Usage>,
}

/// Token usage statistics.
#[derive(Debug, Deserialize)]
pub struct Usage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    #[serde(default)]
    pub cache_creation_input_tokens: Option<u32>,
    #[serde(default)]
    pub cache_read_input_tokens: Option<u32>,
    #[serde(default)]
    pub server_tool_use: Option<ServerToolUse>,
}

/// Server-side tool usage counters (e.g. web search request count).
#[derive(Debug, Deserialize)]
pub struct ServerToolUse {
    #[serde(default)]
    pub web_search_requests: Option<u32>,
}

impl fmt::Display for MessagesResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut first = true;
        // (title, url) pairs collected from text-block citations, de-duplicated.
        let mut sources: Vec<(String, String)> = Vec::new();

        let sep = |f: &mut fmt::Formatter<'_>, first: &mut bool| -> fmt::Result {
            if !*first {
                writeln!(f)?;
            }
            *first = false;
            Ok(())
        };

        for block in &self.content {
            let btype = block.get("type").and_then(Value::as_str).unwrap_or("");
            match btype {
                "text" => {
                    if let Some(t) = block.get("text").and_then(Value::as_str) {
                        sep(f, &mut first)?;
                        write!(f, "{t}")?;
                    }
                    if let Some(cits) = block.get("citations").and_then(Value::as_array) {
                        for c in cits {
                            let url = c.get("url").and_then(Value::as_str).unwrap_or("");
                            let title = c.get("title").and_then(Value::as_str).unwrap_or("");
                            if !url.is_empty() && !sources.iter().any(|(_, u)| u == url) {
                                sources.push((title.to_string(), url.to_string()));
                            }
                        }
                    }
                }
                "thinking" => {
                    if let Some(t) = block.get("thinking").and_then(Value::as_str) {
                        sep(f, &mut first)?;
                        write!(f, "[thinking]\n{t}\n[/thinking]")?;
                    }
                }
                "redacted_thinking" => {
                    sep(f, &mut first)?;
                    write!(f, "[redacted thinking]")?;
                }
                "server_tool_use" => {
                    sep(f, &mut first)?;
                    match block
                        .get("input")
                        .and_then(|i| i.get("query"))
                        .and_then(Value::as_str)
                    {
                        Some(query) => write!(f, "[web search: {query}]")?,
                        None => write!(f, "[server tool use]")?,
                    }
                }
                "web_search_tool_result" => {
                    sep(f, &mut first)?;
                    if let Some(arr) = block.get("content").and_then(Value::as_array) {
                        write!(f, "[{} search results]", arr.len())?;
                    } else if let Some(err) = block
                        .get("content")
                        .and_then(|c| c.get("error_code"))
                        .and_then(Value::as_str)
                    {
                        write!(f, "[web search error: {err}]")?;
                    } else {
                        write!(f, "[search results]")?;
                    }
                }
                "tool_use" => {
                    sep(f, &mut first)?;
                    let name = block.get("name").and_then(Value::as_str).unwrap_or("?");
                    let input = block.get("input").map(|v| v.to_string()).unwrap_or_default();
                    write!(f, "[tool_use: {name}] {input}")?;
                }
                "" => {}
                other => {
                    sep(f, &mut first)?;
                    write!(f, "[{other}]")?;
                }
            }
        }

        if !sources.is_empty() {
            write!(f, "\n\nSources:")?;
            for (title, url) in &sources {
                if title.is_empty() {
                    write!(f, "\n- {url}")?;
                } else {
                    write!(f, "\n- {title} — {url}")?;
                }
            }
        }

        if let Some(reason) = &self.stop_reason {
            write!(f, "\n[stop_reason: {reason}]")?;
        }

        if let Some(usage) = &self.usage {
            write!(
                f,
                "\n[tokens: {} input + {} output = {} total",
                usage.input_tokens,
                usage.output_tokens,
                usage.input_tokens + usage.output_tokens
            )?;
            if let Some(read) = usage.cache_read_input_tokens.filter(|&n| n > 0) {
                write!(f, "; {read} cache read")?;
            }
            if let Some(created) = usage.cache_creation_input_tokens.filter(|&n| n > 0) {
                write!(f, "; {created} cache write")?;
            }
            if let Some(ws) = usage
                .server_tool_use
                .as_ref()
                .and_then(|s| s.web_search_requests)
                .filter(|&n| n > 0)
            {
                write!(f, "; {ws} web searches")?;
            }
            write!(f, "]")?;
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Models API types
// ---------------------------------------------------------------------------

/// The response from listing available models (`GET /v1/models`).
#[derive(Deserialize)]
pub struct ModelsResponse {
    pub data: Vec<ModelInfo>,
}

/// Information about a single model.
#[derive(Deserialize)]
pub struct ModelInfo {
    pub id: String,
    #[serde(default)]
    pub display_name: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn resp(content: Vec<Value>, usage: Option<Usage>) -> MessagesResponse {
        MessagesResponse {
            content,
            stop_reason: Some("end_turn".into()),
            usage,
        }
    }

    fn usage(input: u32, output: u32) -> Usage {
        Usage {
            input_tokens: input,
            output_tokens: output,
            cache_creation_input_tokens: None,
            cache_read_input_tokens: None,
            server_tool_use: None,
        }
    }

    #[test]
    fn display_text_block() {
        let r = resp(
            vec![json!({"type": "text", "text": "Hello!"})],
            Some(usage(10, 5)),
        );
        let s = r.to_string();
        assert!(s.contains("Hello!"));
        assert!(s.contains("[tokens: 10 input + 5 output = 15 total]"));
        assert!(s.contains("[stop_reason: end_turn]"));
    }

    #[test]
    fn display_thinking_then_text() {
        let r = resp(
            vec![
                json!({"type": "thinking", "thinking": "let me think"}),
                json!({"type": "text", "text": "the answer"}),
            ],
            None,
        );
        let s = r.to_string();
        assert!(s.contains("[thinking]"));
        assert!(s.contains("let me think"));
        assert!(s.contains("the answer"));
    }

    #[test]
    fn display_search_with_citations() {
        let r = resp(
            vec![
                json!({"type": "server_tool_use", "name": "web_search", "input": {"query": "rustlang"}}),
                json!({"type": "web_search_tool_result", "content": [{"type": "web_search_result", "url": "https://a.com", "title": "A"}]}),
                json!({"type": "text", "text": "Rust is great", "citations": [{"url": "https://a.com", "title": "A"}]}),
            ],
            None,
        );
        let s = r.to_string();
        assert!(s.contains("[web search: rustlang]"));
        assert!(s.contains("[1 search results]"));
        assert!(s.contains("Sources:"));
        assert!(s.contains("A — https://a.com"));
    }

    #[test]
    fn display_web_search_error() {
        let r = resp(
            vec![json!({"type": "web_search_tool_result", "content": {"type": "web_search_tool_result_error", "error_code": "max_uses_exceeded"}})],
            None,
        );
        assert!(r.to_string().contains("[web search error: max_uses_exceeded]"));
    }

    #[test]
    fn display_empty_content() {
        let r = MessagesResponse {
            content: vec![],
            stop_reason: None,
            usage: None,
        };
        assert_eq!(r.to_string(), "");
    }
}
