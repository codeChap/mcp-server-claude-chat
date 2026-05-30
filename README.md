# mcp-server-claude-chat

An MCP (Model Context Protocol) server for the **Anthropic Claude** Messages API. Built in Rust, it exposes chat, vision, live web search, and model listing as MCP tools — so any MCP client (Claude Desktop, Claude Code, etc.) can consult Claude as a tool.

Communicates via stdio using JSON-RPC 2.0, like all the other MCP servers in this collection. Structurally it mirrors `mcp-server-grok-chat` / `mcp-server-gpt-chat`, adapted to Anthropic's API shape.

## Subscription vs. API credits

This server uses the **Anthropic API with an API key (API credits)**. That is the only supported, terms-compliant way to drive Claude from a third-party tool.

A Claude **Pro/Max subscription** (the kind that powers Claude Code and the Claude apps) is *not* usable here: that authentication is licensed for Anthropic's own first-party clients, and routing it through a custom server would violate Anthropic's Consumer Terms — the "$200 of Claude usage from another service" concern you raised. So: API key it is. (If you run an Anthropic-compatible gateway, point `base_url` at it — see Configuration.)

## Tools

| Tool | Description |
|------|-------------|
| `chat` | Send a message to Claude. Supports multi-turn history, a system prompt, model selection, and **extended thinking** via `thinking_budget`. |
| `chat_with_vision` | Analyse an image given an image URL and a text prompt. |
| `chat_with_search` | Chat with live **web search** — Claude searches server-side and answers with cited sources. |
| `list_models` | List available Claude models and their IDs (cached for 5 minutes). |

> Note: Anthropic has no embeddings endpoint (they recommend Voyage AI), so unlike the grok/gpt servers there is no `embedding` tool.

### chat

| Name | Type | Required | Description |
|------|------|----------|-------------|
| `prompt` | string | yes | The user message to send |
| `system_prompt` | string | no | System prompt to set context/behaviour |
| `messages` | string | no | Conversation history as a JSON array of `{role, content}` objects (roles: `user`/`assistant` only — use `system_prompt` for system instructions) |
| `model` | string | no | Model ID (default: server default, `claude-opus-4-8`). Call `list_models`. |
| `temperature` | float | no | Sampling temperature (0.0–1.0). Ignored when `thinking_budget` is set. |
| `max_tokens` | integer | no | Max tokens to generate (defaults to the server default) |
| `thinking_budget` | integer | no | Enable extended thinking with this token budget (min 1024). Reasoning is returned in a `[thinking]` block; `max_tokens` is auto-raised above the budget. |

### chat_with_vision

| Name | Type | Required | Description |
|------|------|----------|-------------|
| `prompt` | string | yes | Text prompt describing what to analyse |
| `image_url` | string | yes | URL of the image (must be `http://` or `https://`) |
| `model` | string | no | Vision-capable model ID (default: server default) |
| `temperature` | float | no | Sampling temperature (0.0–1.0) |
| `max_tokens` | integer | no | Max tokens to generate |

### chat_with_search

| Name | Type | Required | Description |
|------|------|----------|-------------|
| `prompt` | string | yes | The user message |
| `system_prompt` | string | no | System prompt |
| `model` | string | no | Model ID (default: server default) |
| `temperature` | float | no | Sampling temperature (0.0–1.0) |
| `max_tokens` | integer | no | Max tokens to generate |
| `max_uses` | integer | no | Max web searches Claude may run for this request (default: 5) |

Web search is billed by Anthropic at **$10 per 1,000 searches** on top of token costs, and an org admin must enable web search in the Claude Console.

### list_models

No parameters. Results cached for 5 minutes.

## Prerequisites

- Rust (edition 2024)
- An Anthropic API key from [console.anthropic.com](https://console.anthropic.com/settings/keys)

## Setup

```bash
mkdir -p ~/.config/mcp-server-claude-chat
cp config.toml.example ~/.config/mcp-server-claude-chat/config.toml
# then edit it and set api_key
```

Config (`~/.config/mcp-server-claude-chat/config.toml`):

```toml
api_key = "sk-ant-..."

# Optional:
# base_url = "https://api.anthropic.com/v1"   # or a compatible gateway
# default_model = "claude-opus-4-8"
# default_max_tokens = 4096
```

The server fails fast at startup if the config is missing or `api_key` is empty.

## Build

```bash
cargo build --release   # produces target/release/claude-chat
cargo build             # debug build
cargo run               # run in dev mode
RUST_LOG=debug cargo run
cargo test              # unit tests (response formatting, validation, message building)
```

## MCP Configuration

Claude Desktop (`~/.config/Claude/claude_desktop_config.json`) or any MCP client:

```json
{
  "mcpServers": {
    "claude-chat": {
      "command": "/media/codechap/4TB/develop/mcps/mcp-server-claude-chat/target/release/claude-chat"
    }
  }
}
```

Claude Code:

```bash
claude mcp add claude-chat -- /media/codechap/4TB/develop/mcps/mcp-server-claude-chat/target/release/claude-chat
```

## Project Structure

```
src/
  main.rs    - entry point, config loading, stdio transport setup
  server.rs  - MCP tool definitions (chat, chat_with_vision, chat_with_search, list_models)
  api.rs     - Anthropic HTTP client, Messages/Models types, response formatter
  params.rs  - tool parameter types with serde + JSON Schema derives
  config.rs  - TOML config loading
```

## License

MIT
