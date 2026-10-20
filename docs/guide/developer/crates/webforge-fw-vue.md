---
title: webforge-fw-vue
description: "WebForge: Vue.js support (single-file components, props, emits and usages)."
order: 360
tags: [crate, rust, pack]
---

# webforge-fw-vue

`packs/webforge/fw-vue` — WebForge: Vue.js support (single-file components, props, emits and usages).

## Purpose

WebForge Vue plugin (`forge.vue`): single-file components (`<script setup>` and the
Options API) with their props and emits, and template tag resolution: `<UserCard>` and
`<user-card>` through the script's imports, barrel re-exports, or project-wide (global
registration, Nuxt auto-imports). Go to Declaration on a tag opens the component, on
`:title` / `@select` the prop or emit.

## Main public items

```rust
pub struct Block { … }
pub struct Sfc { … }
pub fn scan_file(path: &str, content: &str) -> Vec<Component> { … }
pub struct VueIndex<'a> { … }
```

## Used by

- IPC: `web_components`, `web_goto`, `web_gutter` with `framework: "vue"`
- UI: `app/ui/src/features/webforge`
- Browser mode: built into `forge-wasm`

## Dependencies

- Workspace crates: [`webforge-lang-web`](webforge-lang-web.md)
- External: none

## Tests

2 source files, about 962 lines; 7 unit tests.

```sh
cargo test -p webforge-fw-vue
```
