---
title: forge-mcp
description: "Forge IDE as a Model Context Protocol server: JSON-RPC, the tool catalog, permissions and approvals, Streamable HTTP and SSE on loopback, and the forge --mcp-stdio proxy."
order: 147
tags: [crate, rust, mcp, ai]
---

# forge-mcp

`crates/forge-mcp` — Forge IDE as a Model Context Protocol server: JSON-RPC, the tool catalog,
permissions and approvals, Streamable HTTP and SSE on loopback, and the `forge --mcp-stdio`
proxy. User documentation: [MCP Server](../../user/mcp-server.md).

## Purpose

External agents connect to the running Forge and call its tools on the open project. The crate
holds everything that is not the IDE itself:

- the protocol: JSON-RPC 2.0 messages (single and batched), `initialize` with version
  negotiation (2025-06-18, 2025-03-26, 2024-11-05), `tools/list`, `tools/call`,
  `resources/list`, `resources/read`, `resources/templates/list`, `prompts/list`,
  `prompts/get`, `ping`;
- the tool catalog with JSON Schemas, groups (`read`, `vcs`, `edit`, `run`, `terminal`,
  `database`), kinds (read, write, exec) and where the IDE answers them (Rust services or the
  editor UI);
- the permission policy: allowed groups, "require approval", Safe Mode for tools that run
  project code, `ai.enabled`, `.aiignore` (arguments are checked, results are filtered) and
  the project boundary;
- sessions, approvals ("allow for this session" is remembered per session, never for terminal
  commands) and the call log;
- the transports: Streamable HTTP on `/mcp` and HTTP+SSE on `/sse` + `/messages`, bound to
  `127.0.0.1`, with a bearer token, loopback `Host` and no foreign `Origin`;
- the stdio proxy for `forge --mcp-stdio` and its discovery file;
- "Copy client configuration" snippets and result adapters (1-based positions).

The IDE side implements `McpHost`: `policy`, `approve`, `call_tool`, `list_resources`,
`read_resource` and `on_event`. In the app that is `app/src-tauri/src/commands/mcp.rs`.

## Modules

| Module | Summary |
| --- | --- |
| `jsonrpc.rs` | JSON-RPC 2.0 requests, notifications and responses, single or batched, with the standard error codes. |
| `protocol.rs` | Version negotiation, the `initialize` result, server instructions, prompts and `forge://project/` resource URIs. |
| `tools.rs` | The tool catalog: names, descriptions, input schemas, groups, kinds, runners and argument checks. |
| `policy.rs` | Whether a call may run (allow, ask, deny) and filtering of results by `.aiignore`. |
| `adapters.rs` | IDE answers turned into tool results: numbered line ranges, Find in Files, symbols, declarations, usages and inspection reports with 1-based positions, text replacement, capped output. |
| `clients.rs` | Client configurations for Claude Code, Claude Desktop, Cursor, Gemini CLI, Codex and VS Code. |
| `engine.rs` | Sessions and the path of every call through the policy, the approval, the host and the call log (`server` feature). |
| `server.rs` | The loopback HTTP server: Streamable HTTP, legacy SSE, token, `Host` and `Origin` checks (`server` feature). |
| `http.rs` | A small HTTP/1.1 subset for the server and its clients, with size limits (`server` feature). |
| `stdio.rs` | The stdio proxy and the discovery file `<config dir>/forge/mcp-server.json` (`server` feature). |
| `token.rs` | The bearer token in the OS keychain, service `forge-mcp` (`keychain` feature). |

## Features

- default `server`: transports, sessions and the proxy (tokio).
- `keychain`: the token in the OS keychain (`keyring`).
- Without features the crate is pure and builds to WebAssembly: browser mode uses
  `mcp_client_configs` from `crates/forge-wasm/src/mcp.rs`.

## Used by

- IPC: `mcp_status`, `mcp_client_configs`, `mcp_regenerate_token`, `mcp_call_log`,
  `mcp_clear_log`, `mcp_answer_approval`, `mcp_tool_result`, `mcp_disconnect`
  (`app/src-tauri/src/commands/mcp.rs`); events `mcp://status`, `mcp://call`,
  `mcp://approval`, `mcp://approval-closed`, `mcp://tool-request`
- The launcher: `forge --mcp-stdio` (`app/src-tauri/src/main.rs`)
- UI: `app/ui/src/features/mcp`
- Browser mode: `app/ui/src/mock/mcpMock.ts`

## Dependencies

- Workspace crates: [`forge-ai-editor`](forge-ai-editor.md) (`.aiignore`)
- External: `serde`, `serde_json`; with `server`: `tokio`, `async-trait`, `getrandom`,
  `tracing`, `dirs`; with `keychain`: `keyring`

## Tests

Protocol, catalog, policy, adapter and client configuration unit tests, plus end-to-end tests
in `tests/e2e.rs` with a fake IDE and an in-process client: Streamable HTTP (lifecycle, tools,
resources, ending sessions), token / `Host` / `Origin` checks, approvals (deny, allow once,
terminal commands asked every time, Safe Mode, the AI switch), the SSE transport, and the stdio
proxy against a running and a stopped server. Only loopback sockets are used.

```sh
cargo test -p forge-mcp
```
