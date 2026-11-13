---
title: forge-scratches
description: "Forge scratch files (PHPStorm's Scratches and Consoles): storage, naming, languages, run commands and the HTTP request client."
order: 230
tags: [crate, rust]
---

# forge-scratches

`crates/forge-scratches` — Forge scratch files (PHPStorm's Scratches and Consoles): storage, naming, languages, run commands and the HTTP request client.

## Purpose

Scratch files, PHPStorm's "Scratches and Consoles".

Scratches are global: they live in the user config dir (`<config>/forge/scratches/`), not
in a project, and are listed at the bottom of the Project tool window.

- `languages`: the language list of the "New Scratch File" popup (file types from
  `forge-filetypes`, ranked with `forge-fuzzy`) and the language → extension mapping.
- `naming`: auto-numbered names (`scratch.php`, `scratch_1.php`, `buffer1.txt`) and name
  validation.
- `store`: create, list, read, write, rename and delete scratches on disk, plus the data
  source attached to SQL scratches.
- `run`: what runs a scratch (PHP with the project's autoloader, Node, ...) and the
  gutter run markers.
- `http`: the `.http` request parser and the console rendering of responses (PHPStorm's
  HTTP Client, lite); `handler` runs response handler scripts (`> {% … %}`) as a safe
  subset without a JavaScript engine; `client` sends the requests (feature `client`).

Everything except `store` and `client` is pure, so browser mode runs it as WebAssembly.

## Modules

| Module | Summary |
| --- | --- |
| `client.rs` | Sends the requests of `.http` scratches (feature `client`). |
| `curl.rs` | cURL commands: `parse_curl` (Import from cURL: shell quoting, `-X`, `-H`, `-d`/`--data-*`, `--json`, `-F`, `-u`, `-b`, `-A`, `-e`, `-G`, `-I`) and `to_curl` (Copy as cURL); `to_http`. |
| `grpc.rs` | `GRPC` requests (feature `client`): unary calls over HTTP/2 (h2, rustls for TLS) with JSON bodies encoded by prost-reflect; message types from server reflection (`grpc.reflection.v1`, then `v1alpha`) or `.proto` files compiled with protox. |
| `oauth.rs` | OAuth 2.0 of `Security.Auth` in the environment files: configs, `$auth.token("id")` references, PKCE (S256), authorization URLs, token request forms and responses (pure). |
| `oauth_client.rs` | The OAuth flows (feature `client`): Client Credentials, Password, Authorization Code with PKCE through the browser and a loopback listener, refresh tokens, a session `TokenCache`. |
| `ws.rs` | `WEBSOCKET` requests (feature `client`, tokio-tungstenite): messages with `=== wait-for-server`, a live event log, console input as messages and Stop as close. |
| `handler.rs` | Response handler scripts of `.http` requests (`> {% … %}`), PHPStorm's HTTP Client API as a small, safe subset: no JavaScript engine, no loops, no access to anything but the response and the global variables. |
| `http.rs` | `.http` request files (PHPStorm's HTTP Client, lite): parsing, `{{variable}}` substitution and the console text of a response. |
| `languages.rs` | Languages of the "New Scratch File" popup (Ctrl+Alt+Shift+Insert): every file type of `forge-filetypes` with an extension, the extension a new scratch gets, and the popup's ordering and speed search. |
| `naming.rs` | Scratch names: PHPStorm numbers new scratches `scratch.php`, `scratch_1.php`, `scratch_2.php`, … and scratch buffers `buffer1.txt` … `buffer5.txt` (the oldest buffer is reused once all five exist). |
| `run.rs` | Running scratches: which program runs a scratch (PHP with the project's Composer autoloader, Node for JavaScript/TypeScript, …), and the gutter ▶ markers of runnable scratches and of the requests in `.http` files. |
| `store.rs` | Scratch files on disk. PHPStorm keeps scratches per user, not per project; Forge stores them in `<config dir>/forge/scratches/` and per-scratch settings (the data source of a SQL scratch) in `<config dir>/forge/scratches.json`. |

## Main public items

```rust
pub use languages::{LanguageItem, LanguageRequest, ScratchLanguage};
pub use run::{Marker, MarkerKind, RunCommand, RunContext, Runner};
pub use store::{ScratchFile, ScratchStore};
pub enum Error { … }
```

## Used by

- IPC: `scratches_list`, `scratches_languages`, `scratches_create`, `scratches_create_buffer`, `scratches_read`, `scratches_write`, `scratches_rename`, `scratches_delete`, `scratches_set_data_source`, `scratches_markers`, `scratches_http_environments`, `scratches_http_run` (`app/src-tauri/src/commands/scratches.rs`)
- UI: `app/ui/src/features/scratches`
- Browser mode: built into `forge-wasm`

## Dependencies

- Workspace crates: [`forge-filetypes`](forge-filetypes.md), [`forge-fuzzy`](forge-fuzzy.md)
- External: `reqwest`, `serde`, `serde_json`, `thiserror`
- Features: `default` = `client`; `client` = `dep:reqwest`

## Tests

8 source files, about 3746 lines; 34 unit tests.

```sh
cargo test -p forge-scratches
```
