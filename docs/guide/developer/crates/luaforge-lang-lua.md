---
title: luaforge-lang-lua
description: "LuaForge: Lua language support (lexer, structural parser, EmmyLua/LuaLS annotations, project index, require resolution, hover, navigation; pure, also built to WebAssembly)."
order: 270
tags: [crate, rust, pack]
---

# luaforge-lang-lua

`packs/luaforge/lang-lua` — LuaForge: Lua language support (lexer, structural parser, EmmyLua/LuaLS annotations, project index, require resolution, hover, navigation; pure, also built to WebAssembly).

## Purpose

LuaForge language pack: Lua 5.1–5.4 and LuaJIT support. Pure Rust (no file system or
processes), so the same code backs the desktop app and the browser mode (WebAssembly).

- `lexer` and `parser` index a file into a `model::FileIndex`: locals with their
  scopes, globals, table fields, functions and methods (`M.f`, `M:f`), references,
  `require` calls and the module's `return`.
- `annotations` parses EmmyLua / LuaLS doc comments (`---@class`, `---@field`,
  `---@param`, `---@return`, `---@type`, `---@alias`, …).
- `require` resolves module names through `package.path`-like templates.
- `project::ProjectIndex` holds a workspace's files and resolves values across them
  (Go to Declaration, Search Everywhere).
- `hover` renders documentation as the `forge-markdown` AST, `structure` builds the
  Structure tool window and `run` finds main chunks for gutter run icons.

## Modules

| Module | Summary |
| --- | --- |
| `annotations.rs` | EmmyLua / LuaLS annotations (`---@class`, `---@field`, `---@param`, `---@return`, `---@type`, `---@alias`, `---@deprecated`, `---@see`) and the Markdown description of a doc comment block. |
| `hover.rs` | Hover documentation from the index and EmmyLua/LuaLS annotations, as the Markdown AST of `forge-markdown` (the same `HoverDoc` the UI renders for language server hovers). |
| `lexer.rs` | Lua 5.1–5.4 / LuaJIT lexer. Error tolerant: unterminated strings and comments end at the end of the line or file, unknown characters become one-character operators. |
| `model.rs` | What the parser extracts from one Lua file. |
| `offsets.rs` | Offset conversion. The parser works in UTF-8 byte offsets; the UI (CodeMirror / JS strings) uses UTF-16 code units. Convert at the IPC boundary. |
| `parser.rs` | Structural Lua parser: a tolerant recursive-descent pass over the tokens that records declarations (locals, parameters, globals, fields, functions and methods) with their scopes, name and field references, `require` calls, the module's `return` value and the doc comments attached to each statement. |
| `project.rs` | The project index: every Lua file of a workspace, with lightweight type resolution across files (locals, globals, table fields, `require`d modules, `---@class` / `---@type` / `---@param` / `---@return` annotations), Go to Declaration and symbol search for Search Everywhere. |
| `require.rs` | `require("a.b")` resolution through `package.path`-like templates relative to the project root (`?.lua`, `?/init.lua`, `lua/?.lua`, …), extended by the `lua.requirePaths` setting and the `runtime.path` of a `.luarc.json`. |
| `run.rs` | Gutter run icons (▶) for main chunks: scripts that do not return a module table, and the `main.lua` of a LÖVE game. |
| `structure.rs` | The Structure tool window (Alt+7) of a Lua file: classes with their fields and methods, module tables with their functions, top-level functions and variables. |

## Main public items

```rust
pub use project::{IndexStats, Location, ProjectIndex, SearchKind, SymbolHit, SymbolKind};
```

## Used by

- IPC: `lua_index_workspace`, `lua_files_changed`, `lua_search_symbols`, `lua_file_structure`, `lua_hover`, `lua_goto`, `lua_run_markers`, `lua_format` (`app/src-tauri/src/commands/lua.rs`)
- UI: `app/ui/src/features/lua`
- Browser mode: built into `forge-wasm`

## Dependencies

- Workspace crates: [`forge-fuzzy`](forge-fuzzy.md), [`forge-markdown`](forge-markdown.md)
- External: `serde`, `serde_json`

## Tests

11 source files, about 4472 lines; 33 unit tests.

```sh
cargo test -p luaforge-lang-lua
```
