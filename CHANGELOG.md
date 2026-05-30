# Changelog

## 0.1.0 — 2026-05-30

Initial release.

### Added
- MCP server for the Anthropic Claude Messages API over stdio (mirrors the
  structure of `mcp-server-grok-chat` / `mcp-server-gpt-chat`).
- Tools:
  - `chat` — chat completion with optional multi-turn history, system prompt,
    model selection, and extended thinking (`thinking_budget`).
  - `chat_with_vision` — image analysis from an image URL + prompt.
  - `chat_with_search` — live web search (server tool `web_search_20250305`)
    with cited sources.
  - `list_models` — lists available Claude models (cached 5 minutes).
- TOML config at `~/.config/mcp-server-claude-chat/config.toml`: `api_key`
  (required), plus optional `base_url`, `default_model`, `default_max_tokens`.
- Response formatter that renders text, thinking, web-search activity, and a
  de-duplicated `Sources:` list, followed by a token-usage line.

### Notes
- Uses the Anthropic **API key** (`x-api-key`), i.e. API credits. A Claude
  Pro/Max subscription is intentionally **not** supported — that auth is for
  Anthropic's first-party clients and reusing it from a third-party server
  violates Anthropic's Consumer Terms.
- No `embedding` tool: Anthropic has no embeddings endpoint (they recommend
  Voyage AI).
- Default model `claude-opus-4-8`; default web search tool `web_search_20250305`
  (the newer `web_search_20260209` requires the code execution tool). Update
  these as Anthropic's lineup changes — `list_models` reports the live set.
