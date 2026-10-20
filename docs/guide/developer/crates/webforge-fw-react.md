---
title: webforge-fw-react
description: "WebForge: React support (JSX components, props and usages)."
order: 340
tags: [crate, rust, pack]
---

# webforge-fw-react

`packs/webforge/fw-react` — WebForge: React support (JSX components, props and usages).

## Purpose

WebForge React plugin (`forge.react`): function and class components with their props,
JSX tag resolution through imports (relative paths, path aliases, barrel re-exports)
and usages. Go to Declaration on `<UserCard` opens the component, on `title=` the prop,
and on a component's own name lists the JSX tags that use it, like WebStorm.

## Main public items

```rust
pub fn is_script(path: &str) -> bool { … }
pub fn scan_file(path: &str, content: &str) -> Vec<Component> { … }
pub struct ReactIndex<'a> { … }
```

## Used by

- IPC: `web_components`, `web_goto`, `web_gutter` with `framework: "react"`
- UI: `app/ui/src/features/webforge`
- Browser mode: built into `forge-wasm`

## Dependencies

- Workspace crates: [`webforge-lang-web`](webforge-lang-web.md)
- External: none

## Tests

2 source files, about 829 lines; 6 unit tests.

```sh
cargo test -p webforge-fw-react
```
