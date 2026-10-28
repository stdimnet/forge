---
title: forge-jsonschema
description: "JSON Schema for JSON, JSON5/JSONC and YAML files: position-keeping parsers, a draft 4–2020-12 validator with PHPStorm's messages, completion, hover, Go to Declaration, schema mappings, bundled schemas and remote schema downloads (pure, WebAssembly-friendly; downloads behind the remote feature)."
order: 116
tags: [crate, rust, json, yaml, json schema]
---

# forge-jsonschema

`crates/forge-jsonschema` — JSON Schema for JSON, JSON5/JSONC and YAML files: position-keeping
parsers, a validator with PHPStorm's messages, completion, hover, Go to Declaration, schema
mappings, bundled schemas and remote schema downloads.

## Purpose

The logic behind [JSON Schema](../../user/json-schema.md) support. The crate is pure (no file
system or network) and also runs in browser mode through `forge-wasm`; the `remote` cargo
feature adds `remote::RemoteCache`, the download cache the desktop app uses. All public answers
use UTF-16 offsets (CodeMirror positions); internally everything is byte offsets.

## Modules

| Module | Summary |
| --- | --- |
| `tree.rs` | The document tree both parsers produce: `Node { value, start, end, custom_tag }` with `Value::{Null, Bool, Number, String, Array, Object(Vec<Property>), Error}`; a `Property` keeps its key range, the colon and an optional value (a key being typed has none). |
| `json.rs` | Tolerant JSON parser: comments, JSON5 (unquoted keys, single quotes, trailing commas, hex, `Infinity`), and unfinished input (`{"a": 1, "na` is a property `na` without colon). |
| `yaml.rs` | Tolerant YAML parser: block mappings and sequences (`- key: v` compact items), flow collections, plain/quoted/block scalars, comments, documents, anchors, aliases, `<<` merge keys (reported at the `<<` key), tags (`!!str`; other tags mark the node as not judged). YAML 1.2 core schema scalars. |
| `uri.rs` | `$id`/`$ref` resolution (RFC 3986 section 5), fragments, JSON pointer decoding; project files are `project:/<path>`. |
| `schema.rs` | `SchemaDoc` (a parsed schema file with its `$id`s, anchors and external refs), `Registry` (loaded documents, aliases, failed URIs, `missing()`), `Sch` (a subschema with its base URI), `Draft`. |
| `bundled.rs` | The bundled schemas (`schemas/*.json`, compressed by `build.rs`, parsed once) and the URLs they are published under, including the official OpenAPI 3.0 and 3.1 schemas (Apache-2.0). |
| `validate.rs` | The validator: every keyword of draft 4–2020-12 (see the user page), best-branch reporting for `anyOf`/`oneOf`, evaluated properties/items for `unevaluated*`, ECMAScript patterns through `regex-lite`. |
| `format.rs` | `format` checks: `date-time`, `date`, `time`, `email`, `ipv4`, `ipv6`, `uuid`, `uri`. |
| `navigate.rs` | Schemas at a document path: `$ref`, `allOf`, the `anyOf`/`oneOf` branches compatible with the written value, `if`/`then`/`else`, `dependentSchemas`; `properties`/`patternProperties`/`additionalProperties`/`items` steps with the declaration's position. |
| `complete.rs` | Completion contexts (JSON from the tree, YAML from indentation) and items: property names (required first) with value templates and commas, values (`enum`, `const`, booleans, `null`, `default`, `examples`, `{}`/`[]`). |
| `mapping.rs` | Which schema a file uses: user mappings, `$schema` / the YAML modeline, built-in mappings, OpenAPI documents by their `openapi:` key, the SchemaStore catalog; glob matching. |
| `remote.rs` | (`remote` feature) `RemoteCache`: downloads into `<cache dir>/forge/json-schemas/<sha256>.json`, fresh for a day, stale copies when offline. |
| `lib.rs` | `validate`, `complete`, `hover`, `goto` on a registry and a root URI, `DocKind`, `load_bundled`. |

## Main public items

```rust
pub fn validate(reg: &Registry, root_uri: &str, kind: DocKind, text: &str) -> Vec<ProblemOut>;
pub fn complete(reg: &Registry, root_uri: &str, kind: DocKind, text: &str, offset: usize) -> Option<CompletionOut>;
pub fn hover(reg: &Registry, root_uri: &str, kind: DocKind, text: &str, offset: usize) -> Option<HoverOut>;
pub fn goto(reg: &Registry, root_uri: &str, kind: DocKind, text: &str, offset: usize) -> Vec<Target>;
pub fn load_bundled(reg: &mut Registry, uri: &str) -> bool;
pub mod mapping { pub fn select(path, text, kind, user: &[UserMapping], catalog: Option<&Catalog>) -> Option<Selection>; }
impl Registry { pub fn insert(&mut self, doc: Arc<SchemaDoc>); pub fn missing(&self) -> Vec<String>; pub fn mark_failed(&mut self, uri: &str, error: String); }
impl SchemaDoc { pub fn parse(uri: &str, location: &str, text: String) -> Result<SchemaDoc, String>; }
```

Loading is the caller's job: insert the root schema, then load what `Registry::missing()`
lists (`$ref`s to other files) until it is empty; unloadable documents are marked failed and
their refs accept anything.

## Used by

- IPC: `json_schema_validate`, `json_schema_complete`, `json_schema_hover`,
  `json_schema_goto`, `json_schema_read`, `json_schema_builtins`
  (`app/src-tauri/src/commands/json_schema.rs`: settings, project files through the backend,
  remote downloads, parsed-schema caches).
- UI: `app/ui/src/features/jsonSchema` (editor problems, completion, hover, Go to Declaration,
  read-only `json-schema://` files, the status bar widget, the settings pages).
- Browser mode: built into `forge-wasm` (`json_schema.rs`).

## Bundled data

`schemas/` holds the schemas of the built-in mappings, unmodified, with their licences in
`schemas/NOTICE` and `schemas/LICENSE-APACHE-2.0`: SchemaStore's `package.json`,
`tsconfig.json`, `jsconfig.json`, `eslintrc.json`, `prettierrc.json`, `github-workflow.json`
and `github-action.json` and the Compose Specification's `compose-spec.json` (Apache-2.0);
Composer's `composer.json` schema and GitLab's `gitlab-ci.json` (MIT).

## Dependencies

- External: `serde`, `serde_json`, `regex-lite` (`pattern`, `patternProperties`), `flate2`
  (bundled schemas); with `remote`: `reqwest` (rustls), `sha2`, `tokio`. All MIT or Apache-2.0.

## Tests

`cargo test -p forge-jsonschema --features remote`: the parsers, URI resolution, every keyword,
completion and hover in JSON and YAML, Go to Declaration, mappings and globs, the bundled
schemas against composer.json, package.json, Compose, GitHub Actions and GitLab CI samples, and
`RemoteCache` against a local HTTP server (no network).
