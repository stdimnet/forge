---
title: webforge-web-tooling
description: "WebForge web tooling: OpenAPI endpoints and previews, GraphQL schemas, validation and completion, Nx workspaces, Nuxt pages and components, Deno tasks (pure, also built to WebAssembly)."
order: 376
tags: [crate, rust, pack]
---

# webforge-web-tooling

`packs/webforge/web-tooling` — WebForge web tooling: OpenAPI endpoints and previews, GraphQL schemas, validation and completion, Nx workspaces, Nuxt pages and components, Deno tasks (pure, also built to WebAssembly).

## Purpose

The pure models behind [Web tooling](../../user/web-tooling.md). The desktop app reads the
project through its backend (`app/src-tauri/src/commands/web_tooling.rs`); browser mode runs
the same code as WebAssembly (`crates/forge-wasm/src/web_tooling.rs`) over the demo files.

## Modules

| Module | Summary |
| --- | --- |
| `openapi.rs` | OpenAPI 3.x / Swagger 2.0 documents (YAML or JSON through `forge-jsonschema`'s parsers): `version`/`is_openapi` by the top-level key, `endpoints` (method, path, operationId, summary, tags, line), `preview` (servers, operations grouped by tag with parameters, request body and responses, examples generated from schemas with local `$ref`s, schemas) and `http_request` ("Generate HTTP Request" as `.http` text). |
| `graphql.rs` | GraphQL with apollo-compiler (MIT / Apache-2.0): `SchemaSet` (SDL files, schema problems, `validate` of operations, `complete` of fields, arguments, types and keywords from a token scan of the text before the caret), `syntax_problems`, `introspection_to_sdl`, `parse_config` for `graphql.config.*` / `.graphqlrc*` (schema files, documents, endpoints with headers) and `request_body`. Offsets are UTF-16. |
| `nx.rs` | Nx workspaces: `workspace` reads `nx.json` (`targetDefaults`, `defaultBase`), `project.json` files and workspace `package.json` files (the `nx` key, scripts as targets) into projects with kind, tags, targets (executor or command, line) and dependencies (`implicitDependencies` and dependencies between workspace packages); `run_args`. |
| `nuxt.rs` | Nuxt: `routes` from `pages/` (index, `[id]`, `[[optional]]`, `[...slug]`, route groups, Nuxt 4's `app/`), `components` from `components/` with path prefixes collapsed like Nuxt, `component_for_tag` (`<base-button>`, `<LazyX>`). |
| `deno.rs` | Tasks of `deno.json` / `deno.jsonc` (string and object tasks). |

## Main public items

```rust
pub mod deno;
pub mod graphql;
pub mod nuxt;
pub mod nx;
pub mod openapi;
```

## Dependencies

`serde`, `serde_json`, `forge-jsonschema` (YAML/JSON trees with positions), `apollo-compiler`
(GraphQL parsing and validation).

## Tests

Unit tests in each module: endpoint extraction and the preview of a Pet Store document,
Swagger 2.0, Generate HTTP Request, GraphQL validation (unknown fields, missing arguments,
syntax errors), schema problems, completion contexts, introspection to SDL, config files, Nx
projects and graph, Nuxt routes and components, Deno tasks.
