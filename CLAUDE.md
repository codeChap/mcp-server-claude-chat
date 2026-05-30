# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What This Is

An MCP (Model Context Protocol) server that wraps the **Anthropic Claude Messages API**, exposing chat, vision, live web search, and model listing as MCP tools over stdio. It mirrors the structure of `mcp-server-grok-chat` / `mcp-server-gpt-chat`, adapted to Anthropic's API conventions.

## Build & Run

```bash
cargo build --release    # binary: target/release/claude-chat
cargo build              # debug build
cargo run                # dev mode
RUST_LOG=debug cargo run # debug logging (to stderr; stdout is reserved for JSON-RPC)
cargo test               # unit tests
```

## Configuration

Config file: `~/.config/mcp-server-claude-chat/config.toml`

```toml
api_key = "sk-ant-..."        # required
# base_url = "https://api.anthropic.com/v1"   # optional override (gateway/proxy)
# default_model = "claude-opus-4-8"           # optional
# default_max_tokens = 4096                    # optional
```

Loaded at startup in `src/config.rs`; the server fails immediately if missing or if `api_key` is empty.

## Auth model (important)

Uses the Anthropic **API key** (`x-api-key` header) — i.e. API credits. A Claude Pro/Max **subscription cannot** be used: that auth is for Anthropic's first-party clients only, and reusing it from a custom server violates the Consumer Terms. The `base_url` field exists so the same code can target an Anthropic-compatible gateway if desired.

## How Anthropic differs from the OpenAI/xAI shape (vs. grok-chat/gpt-chat)

These are the deliberate deviations from the sibling servers:

- **Auth headers:** `x-api-key` + `anthropic-version: 2023-06-01` (not `Authorization: Bearer`).
- **`system` is top-level**, not a message. The `messages` array accepts only `user`/`assistant` roles — `build_messages` rejects anything else.
- **`max_tokens` is required** by the API. We always send it, defaulting to `default_max_tokens` (4096) when the caller omits it.
- **Temperature range is 0.0–1.0** (not 0–2). `validate_temperature` enforces this.
- **No embeddings endpoint** → no `embedding` tool (Anthropic points to Voyage AI).
- **Extended thinking:** `thinking_budget` adds `{"thinking": {"type": "enabled", "budget_tokens": N}}`. Thinking is incompatible with a custom `temperature` (we drop it), requires `budget_tokens >= 1024`, and needs `max_tokens > budget_tokens` (we auto-raise `max_tokens`).
- **Response is content blocks**, kept as `Vec<serde_json::Value>` so new block types don't break parsing. `MessagesResponse`'s `Display` renders `text`, `thinking`, `server_tool_use`, `web_search_tool_result`, and `tool_use`, then appends a de-duplicated `Sources:` list from text-block citations and a token/usage line.
- **Web search** is a server tool: `{"type": "web_search_20250305", "name": "web_search", "max_uses": N}`. Claude runs the searches server-side and returns cited sources.

## Architecture

Five source files, no sub-crates:

- **`main.rs`** — entry point. Loads config, builds `AnthropicClient`, builds `ClaudeServer`, starts rmcp stdio transport.
- **`config.rs`** — TOML config from `~/.config/mcp-server-claude-chat/config.toml`. `api_key` (required), `base_url` (defaulted), `default_model`/`default_max_tokens` (optional).
- **`api.rs`** — `AnthropicClient` (reqwest, generic `request<Req,Resp>()` adding the Anthropic headers), all API types (`MessagesRequest`, `Message`, `Thinking`, `MessagesResponse`, `Usage`, `ModelsResponse`, …), and the `Display` formatter for responses.
- **`server.rs`** — MCP tools via rmcp `#[tool]`/`#[tool_router]`/`#[tool_handler]`. Tools: `chat`, `chat_with_vision`, `chat_with_search`, `list_models`. Shared helpers: `validate_temperature`, `build_messages`, `resolve_model`, `do_messages`.
- **`params.rs`** — serde + `JsonSchema` parameter structs; `#[schemars(description)]` becomes the MCP tool parameter docs.

## Key Constants (`server.rs`)

- Default model: `claude-opus-4-8`
- Default max tokens: `4096`
- Min thinking budget: `1024`
- Web search tool type: `web_search_20250305` — the stable version. The newer `web_search_20260209` adds dynamic filtering but additionally requires the code execution tool; update this constant if Anthropic ships a newer stable version.
- API base URL: `https://api.anthropic.com/v1` (default in `config.rs`)
- API version header: `2023-06-01` (in `api.rs`)
- HTTP timeout: 300 seconds (in `api.rs`)

## Adding a New Tool

1. Add a parameter struct to `params.rs` (`Deserialize` + `JsonSchema`).
2. Add a `#[tool(description = "...")]` method inside the `#[tool_router] impl ClaudeServer` block in `server.rs`.
3. Add any new API types to `api.rs` if calling a new endpoint.

## Dependencies

`rmcp` v1.2 for MCP protocol handling, `reqwest` for HTTP, `moka` for the models cache. Rust edition 2024.
