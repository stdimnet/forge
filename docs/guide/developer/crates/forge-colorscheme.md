---
title: forge-colorscheme
description: "Editor color schemes (Settings | Editor | Color Scheme): the Ember and Anvil schemes, user schemes stored in the settings, per-attribute text attributes, CSS for the editor, and IntelliJ .icls import/export (pure, WebAssembly-friendly)."
order: 35
tags: [crate, rust]
---

# forge-colorscheme

`crates/forge-colorscheme` — Editor color schemes (Settings | Editor | Color Scheme): the Ember and Anvil schemes, user schemes stored in the settings, per-attribute text attributes, CSS for the editor, and IntelliJ .icls import/export (pure, WebAssembly-friendly).

## Purpose

Everything behind the Color Scheme page; the UI (`app/ui/src/features/colorScheme`) only
renders the snapshot and sets the CSS variables it carries.

- `model`: `TextAttributes` (foreground, background, bold, italic, effect and effect color)
  with the settings encoding, the attribute list (`ATTRIBUTES`: General and Language Defaults,
  which fields each one has, its CSS target and the IntelliJ names it maps to), the bundled
  schemes `Bundled::Ember` / `Bundled::Anvil` (the values of `theme.css`) and the CSS of an
  attribute.
- `scheme`: `SchemeSettings` read from `editor.colorScheme.active` / `.schemes`, resolution
  (follow the theme, bundled or user scheme), `apply(SchemeChange)` returning settings writes
  (select, duplicate, rename, delete, set or reset an attribute, reset all), the
  `SchemesSnapshot` of the page and the document CSS variables.
- `icls`: `import_icls` (IntelliJ `.icls`: `<colors>`, `<attributes>`, `baseAttributes`,
  `FONT_TYPE`, `EFFECT_TYPE`, Darcula → Ember, others → Anvil) and `export_icls`.
- `preview`: the sample code of the page as segments by attribute.

## Main public items

```rust
pub fn import_icls(text: &str) -> Result<Imported, String> { … }
pub fn export_icls(settings: &SchemeSettings, name: &str) -> String { … }
impl SchemeSettings {
    pub fn snapshot(&self, dark: bool) -> SchemesSnapshot { … }
    pub fn apply(&self, change: SchemeChange, dark: bool) -> Vec<SettingWrite> { … }
    pub fn add_imported(&self, scheme: UserScheme) -> Vec<SettingWrite> { … }
}
```

## Used by

- IPC: `color_scheme_state`, `color_scheme_change`, `color_scheme_import`,
  `color_scheme_export` (`app/src-tauri/src/commands/color_scheme.rs`).
- UI: `app/ui/src/features/colorScheme`, `app/ui/src/editor/forgeTheme.ts` (reads the
  variables).
- Browser mode: built into `forge-wasm` (`colorscheme.rs`).

## Dependencies

- Workspace crates: none
- External: `serde`, `serde_json`, `quick-xml`

## Tests

Unit tests for color normalization, the attribute encoding, the bundled schemes, CSS of
effects, scheme changes and their settings round trip, .icls import (colors, fonts, effects,
`baseAttributes`, parents) and export/import round trips.
