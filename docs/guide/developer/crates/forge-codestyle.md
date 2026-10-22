---
title: forge-codestyle
description: "Code style schemes (Settings | Editor | Code Style): options per language, presets, scheme storage and resolution, import/export and the formatting passes and previews (pure, WebAssembly-friendly)."
order: 36
tags: [crate, rust, code style, formatting]
---

# forge-codestyle

`crates/forge-codestyle` — Code style schemes (Settings | Editor | Code Style): options per language, presets, scheme storage and resolution, import/export and the formatting passes and previews (pure, WebAssembly-friendly).

## Purpose

Everything behind Settings | Editor | Code Style, as in PHPStorm. The UI
(`app/ui/src/features/codeStyle`) renders the pages from the model and keeps the edited
schemes as a draft of the Settings dialog; everything else happens here.

- `model`: the options of the General page and of the 13 language pages (PHP, HTML,
  CSS/SCSS/Less, JavaScript, TypeScript, JSON, YAML, Lua, Blade, Twig, SQL, XML, Markdown).
  Each option has a key (`spaces.afterComma`), title, tab ("Tabs and Indents", "Spaces",
  "Wrapping and Braces", "Blank Lines" and language tabs such as "Code Conversion",
  "Punctuation", "Case", "Other"), section, kind (bool, int range, choice, text), default and
  its name in PHPStorm's XML. Languages list the file types they format (`scss` → CSS,
  `tsx` → TypeScript) and their "Set from…" presets: PSR-12, PER Coding Style 2.0, Symfony,
  Laravel, WordPress and Drupal for PHP; JavaScript Standard Style, Airbnb-like and Prettier
  defaults for JavaScript and TypeScript; stylelint-config-standard; StyLua defaults; …
- `scheme`: where schemes live. Values are keyed `<language>.<option>`. The **Default**
  scheme is the user settings layer under the keys every page always used
  (`php.codeStyle.indent`, `editor.codeStyle.hardWrapAt`), the **Project** scheme the same
  keys in the project layer (options it does not set come from Default), further IDE schemes
  are stored under `editor.codeStyle.schemes.<id>.name` / `.values.<language>.<option>`.
  `editor.codeStyle.scheme` selects the project's scheme; without it a project with code
  style settings uses its Project scheme. `load` builds the schemes from the layers,
  `selected_values` gives the values formatting uses, `effective_settings` replaces the code
  style keys of the effective settings with them (for `forge_text::resolve_format` and the
  PHP code style), `plan_save` turns edited schemes into settings writes (only values that
  differ from the scheme's base, removed schemes deleted) and `apply_preset` is "Set from…".
  The single PHP brace option of older versions (`php.codeStyle.braces`: `psr12`, `allman`,
  `kr`) is read as the three brace options and removed on the next save.
- `io`: export and import. Forge's JSON (`{ "forgeCodeStyle": 1, "name", "values" }`) has every
  option; the PHPStorm XML subset (`<code_scheme>` with root options, `<…CodeStyleSettings>`,
  `<codeStyleSettings language="…">` and `<indentOptions>`, also inside
  `.idea/codeStyles/Project.xml`) maps Forge's options to PHPStorm's names, and options
  PHPStorm has no name for go into a `<ForgeCodeStyleSettings>` element so a scheme survives
  the round trip. The XML reader is a small built-in parser.
- `format`: a lossless lexer (`format::lexer`) for PHP, JavaScript/TypeScript, CSS, JSON,
  Lua and SQL, and the formatters:
  - `preview(language, values)` lays the page's sample out completely with the options
    (indentation from the nesting, brace placement, spaces, blank lines, conversions; markup
    by element/directive nesting, YAML by nesting, SQL clauses, Markdown blocks). The
    General page's sample shows the formatter markers.
  - `post_format(request, values)` runs after a language server or tool formatted a file:
    for PHP the spaces, brace and `else`/`catch`/`finally` placement and the code conversions
    (array syntax, trailing commas, `true`/`false`/`null` case) on the existing lines (it
    never re-indents), quotes in plain JavaScript/TypeScript files (not JSX), hex color case in
    CSS, SQL keyword case, the Markdown spaces and blank lines, Blade/Twig delimiters, blank
    lines, indents on empty lines and the `@formatter:off` / `@formatter:on` regions, which get
    their text from before formatting.
- `lsp`: the options handed to external formatters: `settings_patch(language, values)` for
  the language servers (`intelephense.format.braces`, `typescript.format` /
  `javascript.format` and the quote preference, `css`/`scss`/`less.format`, `html.format`,
  `json.format.keepLines`, `yaml.format`, `xml.format`, `Lua.format.defaultConfig`) and
  `stylua_args(values)` for StyLua when the project has no `stylua.toml`.

## Main public items

```rust
pub fn model() -> &'static CodeStyleModel { … }
pub fn load(layers: Layers<'_>) -> SchemesState { … }
pub fn selected_values(layers: Layers<'_>) -> Values { … }
pub fn effective_settings(effective: &Map<String, Value>, layers: Layers<'_>) -> Map<String, Value> { … }
pub fn plan_save(layers: Layers<'_>, state: &SchemesState) -> Result<Vec<SettingWrite>, String> { … }
pub fn apply_preset(values: &Values, language: &str, preset: &str) -> Option<Values> { … }
pub fn export(name: &str, values: &Values, format: ExportFormat) -> String { … }
pub fn import(text: &str) -> Result<ImportedScheme, String> { … }
pub fn preview(language: &str, values: &Values) -> Option<String> { … }
pub fn post_format(req: &PostFormatRequest, values: &Values) -> String { … }
pub fn lsp::settings_patch(language: &str, values: &Values) -> Value { … }
pub fn lsp::stylua_args(values: &Values) -> Vec<String> { … }
```

## Used by

- IPC (`app/src-tauri/src/commands/code_style.rs`, client `app/ui/src/ipc/codeStyle.ts`):
  `code_style_model`, `code_style_schemes`, `code_style_save({ schemes })`,
  `code_style_preview({ language, values })`, `code_style_export({ name, values, format,
  path? })`, `code_style_import({ text?, path? })` and `code_style_post_format({ request })`
  (registered as the last format post-processor of Reformat Code).
- `editor_file_format` resolves indents with the selected scheme (`effective_settings`, file
  types mapped to their code style language); the PHP code style (braces, blank lines) and
  Intelephense's settings read it too; `lsp_format({ …, languageId })` pushes
  `settings_patch` to the file's server (`LspClient::update_settings`); `lua_format` passes
  `stylua_args`.
- Browser mode: built into `forge-wasm` (`codestyle.rs`); the mocks in
  `app/ui/src/mock/codeStyleMock.ts` store schemes in the mock settings layers.

## Dependencies

- Workspace crates: none
- External: `serde`, `serde_json`

## Tests

Unit tests for the model (tabs, presets, sanitizing), scheme storage (legacy keys and the
brace migration, Project inheriting from Default, IDE schemes, selection, removal, duplicate
names), JSON and XML round trips and a PHPStorm project file, the lexer (lossless for every
language), the layout of PHP (PSR-12, WordPress), JavaScript (Standard, Prettier), CSS, JSON,
Lua, markup, YAML, SQL and Markdown, the PHP spacing pass (alignment kept, braces moved,
keyword method names, attributes, array conversion), the formatter markers, the language
server settings and StyLua's command line. The PHP spacing pass was also checked on several
hundred PHP files (stubs and fixtures) with every preset: idempotent, and `php -l` stays
clean.
