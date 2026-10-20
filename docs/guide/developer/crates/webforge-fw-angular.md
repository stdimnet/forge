---
title: webforge-fw-angular
description: "WebForge: Angular support (components, directives, pipes, templates and styles)."
order: 340
tags: [crate, rust, pack]
---

# webforge-fw-angular

`packs/webforge/fw-angular` — WebForge: Angular support (components, directives, pipes, templates and styles).

## Purpose

WebForge Angular plugin (`forge.angular`): `@Component`, `@Directive` and `@Pipe`
classes with selectors, inputs and outputs (decorators and signal functions), their
`templateUrl`/`template` and `styleUrls`/`styleUrl`. Navigation like WebStorm's:
element and attribute selectors, `[input]`/`(output)` bindings and `| pipe`s in
templates go to their declarations; `templateUrl` and `styleUrls` strings open the
files; line markers link a component class and its template and styles.

## Main public items

```rust
pub struct Declaration { … }
pub fn scan_file(path: &str, content: &str) -> Vec<Declaration> { … }
pub struct AngularIndex<'a> { … }
```

## Used by

- IPC: `web_components`, `web_goto`, `web_gutter` with `framework: "angular"`
- UI: `app/ui/src/features/webforge`
- Browser mode: built into `forge-wasm`

## Dependencies

- Workspace crates: [`webforge-lang-web`](webforge-lang-web.md)
- External: none

## Tests

2 source files, about 1000 lines; 6 unit tests.

```sh
cargo test -p webforge-fw-angular
```
