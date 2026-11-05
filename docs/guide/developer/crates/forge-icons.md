---
title: forge-icons
description: "File icon themes for Forge: the theme format, its validation and the icon resolver (pure, WebAssembly-friendly)."
order: 80
tags: [crate, rust]
---

# forge-icons

`crates/forge-icons` — File icon themes for Forge: the theme format, its validation and the icon resolver (pure, WebAssembly-friendly).

## Purpose

The model behind [icon themes](../icon-themes.md): which icon of a theme a file, folder, code
symbol or UI place gets. Every themed icon in the UI asks the resolver of the active theme
first and falls back to Forge's own icons without an answer.

| Item | Summary |
| --- | --- |
| `IconThemeDef` | `icon-theme.json` (format 1 and 2, `FORMAT_VERSION`): `icons`, `iconsLight`, defaults (`file`, `folder`, `folderExpanded`, `rootFolder`, `rootFolderExpanded`, `folderHollow`, `folderHollowExpanded`), `fileNames`, `fileExtensions`, `fileTypes`, `pathRules`, `filePatterns`, `fileRegex`, `folderNames`, `folderNamesExpanded`, `symbols`, `ui`, `options`; `parse`, `validate` (version, paths, references, regexes, options), `warnings` (unknown symbol kinds and UI ids), `svg_files` (SVG keys, light variants as `<id>_light`) |
| `IconThemeOptions`, `TreeArrows` | the look a theme asks for: tree arrows (`chevron`, `triangle`, `plusMinus`, `arrow`, `none`), accent color, monochrome, icon size, hollow folders |
| `Association`, `AssociationKind` | the user's own rules (`fileIcons.associations`): file name, extension, pattern, regex, folder name |
| `IconResolver` | `new(theme, associations)` (bad associations are skipped and reported), `resolve(path, IconKind)`, `lookup(&Lookup)` (root, hollow, detected file type) with a cache per path, `needs_file_type`, `symbol(kind, SymbolModifiers)` → `SymbolIcon { icon, markers }`, `ui(id)`, `set_light`, `set_project_root` (both clear the cache) |
| `ids` | the stable ids: `SYMBOL_KINDS`, `symbol_fallback`, `UI_PREFIXES`, `UI_ICONS`, `ui_fallbacks` |
| `tint` | tint layers: `has_tint_layer`, `tint_svg(svg, color)` (`data-forge-tint`, class `forge-tint`) |
| `folders` | `FolderColors` (`fileIcons.folderColors`, cached per path), `path_glob` (relative to the project root, `/` anchors), `folders_with_open_files` (hollow folders) |
| `uses_light_variant(ui_theme, choice)` | light variants with `light` and `*-light` UI themes, or as `fileIcons.variant` forces |
| `check_svg`, `is_svg_path`, `glob_match`, `relative_path` | SVG limits and safety checks, package paths, globs (`*`, `**`, `?`, `[a-z]`, `[!x]`), project-relative paths |

Matching order for files: associations, path rules, file names (exact, then ignoring case),
patterns, regular expressions, extensions (longest compound first), the detected file type,
the default icon. Folders: associations, the root folder icons, expanded or closed names,
defaults; then the hollow and the light variant.

## Used by

- `forge-plugins` (`icon_themes::load`, install checks) and through it the IPC command
  `icon_theme_load`
- `forge-wasm` (`icons_set_theme`, `icons_set_options`, `icons_resolve`,
  `icons_needs_file_type`, `icons_symbol`, `icons_ui`, `icons_folder_color`, `icons_tint`,
  `icons_has_tint_layer`, `icons_open_folders`, `icons_validate_theme`): the UI resolves icons
  synchronously with it in the desktop app and in browser mode
- UI: `app/ui/src/features/iconThemes`, `app/ui/src/shell/FileIcon.tsx`,
  `app/ui/src/features/php/SymbolIcon.tsx`

## Dependencies

- Workspace crates: none
- External: `serde`, `serde_json`, `regex-lite`

## Tests

Theme validation (format 1 and 2), file and folder matching order, path rules and file types,
root, hollow and light folders, symbols with combined kinds, fallbacks and markers, UI ids
with fallbacks, user associations, skipped associations, the per-path cache, folder colors,
open-file folders, tinting, globs and SVG checks.

```sh
cargo test -p forge-icons
```
