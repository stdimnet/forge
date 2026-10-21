---
title: forge-file-templates
description: "File and code templates (Settings | Editor | File and Code Templates): a Velocity subset renderer, the built-in templates, PSR-4 namespace mapping and New-file rendering (pure, WebAssembly-friendly)."
order: 76
tags: [crate, rust]
---

# forge-file-templates

`crates/forge-file-templates` — File and code templates (Settings | Editor | File and Code Templates): a Velocity subset renderer, the built-in templates, PSR-4 namespace mapping and New-file rendering (pure, WebAssembly-friendly).

## Purpose

Everything behind File | New ▸ <template> and the code templates of the PHP Generate actions,
shared by the desktop backend and the browser-mode WebAssembly build.

- `velocity`: the template language, the part of Apache Velocity PHPStorm's templates use
  (`${NAME}`, `$!{X}`, string methods, `#if`/`#elseif`/`#else`/`#end`, `#set`, `#parse`,
  comments, `#[[unparsed]]#`). Whole-line directives leave no line behind ("lines" whitespace
  mode); syntax errors carry their line. `referenced_variables` lists what a template uses.
- `builtin`: the shipped templates (PHP File/Class/Interface/Trait/Enum, PHPUnit Test,
  Laravel Controller/Model, Symfony Controller, HTML, CSS, JavaScript, TypeScript, Vue SFC,
  React (JSX/TSX), Lua; the PHP File Header include; the PHP code templates), each with the
  project type it needs (`when`) and its menu group.
- `psr4`: PSR-4 rules from composer.json (`autoload` and `autoload-dev`, relative to the
  composer.json folder) and the mapping folder → namespace (deepest folder wins) and
  namespace → folder (longest prefix wins).
- `catalog` merges the built-ins with the user's templates from the `fileTemplates.custom`
  setting; `new_file` renders a New request (name, sub folders, extension, namespace, Extends /
  Implements with `use` imports, date/user/project variables) into a path and text;
  `render_code` renders a code template for `phpforge-lang-php`'s generators.

## Main public items

```rust
pub fn render(template: &str, vars: &Vars, includes: Includes<'_>) -> Result<String, TemplateError> { … }
pub fn catalog(custom: &[CustomTemplate]) -> Vec<TemplateInfo> { … }
pub fn new_file(req: &NewFileRequest, custom: &[CustomTemplate]) -> Result<NewFile, TemplateError> { … }
pub fn render_code(overrides: &BTreeMap<String, String>, name: &str, vars: &Vars) -> Result<String, TemplateError> { … }
pub fn psr4::namespace_for_dir(roots: &[Psr4Root], dir: &str) -> Option<String> { … }
pub fn psr4::dir_for_namespace(roots: &[Psr4Root], namespace: &str) -> Option<String> { … }
```

## Used by

- IPC: `templates_list`, `templates_validate`, `templates_php_namespace`,
  `templates_php_directory`, `templates_create` (`app/src-tauri/src/commands/templates.rs`;
  composer.json is read and the file written through the project backend)
- `phpforge-lang-php`: Implement/Override Methods bodies and PHPDoc comments
  (`GenerateOptions::code_templates`, filled by `php_generate` from the settings)
- UI: `app/ui/src/features/fileTemplates` (New menus, `NewElement` popup, New PHP Class
  dialog, Settings | Editor | File and Code Templates)
- Browser mode: built into `forge-wasm` (`templates_*`)

## Dependencies

- Workspace crates: none
- External: `serde`, `serde_json`

## Tests

4 source files; unit tests for references, whole-line and inline directives, conditions,
`#set`/`#parse` and include recursion, syntax errors with lines, PSR-4 mapping in both
directions, every built-in file template, `use` imports for Extends/Implements, context
variables, catalog merging and code templates.

```sh
cargo test -p forge-file-templates
```
