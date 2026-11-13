---
title: webforge-lang-web
description: "WebForge: package.json, npm scripts, package managers and the shared JavaScript/TypeScript/HTML scanning used by the framework plugins."
order: 370
tags: [crate, rust, pack]
---

# webforge-lang-web

`packs/webforge/lang-web` — WebForge: package.json, npm scripts, package managers and the shared JavaScript/TypeScript/HTML scanning used by the framework plugins.

## Purpose

WebForge core (`forge.web`): `package.json` and npm scripts, package manager detection,
and the lexical scanning of JavaScript/TypeScript modules and HTML/JSX tags that the
React, Vue and Angular plugins (`webforge-fw-*`) build their component indexes on.

Everything except `fs` is pure, so the browser-mode UI runs the same code as
WebAssembly (`crates/forge-wasm/src/web.rs`).

## Modules

| Module | Summary |
| --- | --- |
| `aliases.rs` | `resolve.alias` of `vite.config.*` / `webpack.config.*` read statically: object and array forms, `path.resolve`/`path.join`, `fileURLToPath(new URL(…))`, template strings, root-relative strings, Webpack's exact `name$`. |
| `import_paths.rs` | The import specifier under the caret: Go to Declaration and path completion through relative paths, `tsconfig` paths and bundler aliases. |
| `component.rs` | The component model shared by the React, Vue and Angular plugins, and the gutter marks and navigation answers they return to the UI. |
| `fs.rs` | Loading `Sources` from disk (desktop app only; the browser mock passes its demo files). |
| `imports.rs` | ES module imports and re-exports, read lexically from JavaScript/TypeScript source. |
| `js.rs` | Lexical helpers for JavaScript/TypeScript: matching brackets, top-level members of object literals, type literals and destructuring patterns, string literals. |
| `json.rs` | A small JSON reader that keeps byte offsets, so `package.json` scripts can get gutter icons on their lines. Accepts what `tsconfig.json` allows too: comments and trailing commas. |
| `markup.rs` | Tags and attributes in HTML templates (Vue `<template>`, Angular templates) and JSX. A lexical scanner, not a full parser: it finds `<Name attr=… {expr}>` and keeps byte ranges, which is what navigation needs. |
| `npm.rs` | The npm tool window and `package.json` gutter: every `package.json` of the project with its scripts and package manager. |
| `package_json.rs` | `package.json`: name, scripts with their positions, dependencies, workspaces and the `packageManager` field. |
| `pm.rs` | Package manager detection, like WebStorm's "Package manager: auto": the `packageManager` field (Corepack), else the lock file next to the `package.json` or in a parent directory (workspaces), else npm. |
| `source.rs` | The project files the framework indexes read, and module resolution between them (relative specifiers, `tsconfig.json`/`jsconfig.json` path aliases, index files and workspace packages imported by their package name). |
| `text.rs` | Offsets and positions. Scanners work in UTF-8 byte offsets; the UI (CodeMirror, JS strings) uses UTF-16 code units and 0-based lines/columns. Convert at the boundary. |

## Main public items

```rust
pub use component::{Component, ComponentKind, Framework, GutterKind, GutterMark, Member};
pub use npm::NpmPackage;
pub use package_json::{PackageJson, Script};
pub use pm::{Detection, PackageManager};
pub use source::{SourceFile, Sources};
pub use text::Location;
pub trait FrameworkIndex { … }
```

## Used by

- IPC: `npm_packages`, `npm_gutter` (`app/src-tauri/src/commands/web.rs`)
- UI: `app/ui/src/features/webforge`
- Browser mode: built into `forge-wasm`

## Dependencies

- Workspace crates: none
- External: `ignore`, `serde`, `serde_json`
- Features: `default` = `fs`; `fs` = `dep:ignore`

## Tests

12 source files, about 2546 lines; 29 unit tests.

```sh
cargo test -p webforge-lang-web
```
