use schemars::JsonSchema;
use serde::Deserialize;

/// Parameters for the `chat` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ChatParams {
    #[schemars(description = "The user message / prompt to send to Claude")]
    pub prompt: String,

    #[schemars(description = "Optional system prompt to set context/behaviour")]
    pub system_prompt: Option<String>,

    #[schemars(
        description = "Full conversation history as a JSON array of {role, content} objects, \
                       where role is \"user\" or \"assistant\". When provided, 'prompt' is \
                       appended as the final user message. Use 'system_prompt' for system \
                       instructions — Anthropic does not accept a \"system\" role here."
    )]
    pub messages: Option<String>,

    #[schemars(
        description = "Model ID. Defaults to the server default (claude-opus-4-8). \
                       Call the list_models tool for the current set of available models."
    )]
    pub model: Option<String>,

    #[schemars(
        description = "Sampling temperature (0.0 - 1.0). Ignored when thinking_budget is set."
    )]
    pub temperature: Option<f32>,

    #[schemars(
        description = "Maximum tokens to generate. The Anthropic API requires this; the server \
                       default is used when omitted."
    )]
    pub max_tokens: Option<u32>,

    #[schemars(
        description = "Enable extended thinking with this token budget (minimum 1024). Claude \
                       reasons internally before answering and the reasoning is returned in a \
                       delimited [thinking] block. max_tokens is automatically raised above the \
                       budget when needed, and temperature is ignored while thinking is enabled."
    )]
    pub thinking_budget: Option<u32>,
}

/// Parameters for the `chat_with_vision` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct VisionParams {
    #[schemars(description = "Text prompt describing what to analyse in the image")]
    pub prompt: String,

    #[schemars(description = "URL of the image to analyse (must be http:// or https://)")]
    pub image_url: String,

    #[schemars(
        description = "Model ID. Defaults to the server default. Must be a vision-capable model. \
                       Call the list_models tool for the current set of available models."
    )]
    pub model: Option<String>,

    #[schemars(description = "Sampling temperature (0.0 - 1.0)")]
    pub temperature: Option<f32>,

    #[schemars(description = "Maximum tokens to generate")]
    pub max_tokens: Option<u32>,
}

/// Parameters for the `chat_with_search` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchParams {
    #[schemars(description = "The user message / prompt")]
    pub prompt: String,

    #[schemars(description = "Optional system prompt")]
    pub system_prompt: Option<String>,

    #[schemars(
        description = "Model ID. Defaults to the server default. \
                       Call the list_models tool for the current set of available models."
    )]
    pub model: Option<String>,

    #[schemars(description = "Sampling temperature (0.0 - 1.0)")]
    pub temperature: Option<f32>,

    #[schemars(description = "Maximum tokens to generate")]
    pub max_tokens: Option<u32>,

    #[schemars(
        description = "Maximum number of web searches Claude may run for this request (default: 5)"
    )]
    pub max_uses: Option<u32>,
}
