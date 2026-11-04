---
title: forge-icons
description: "File icon themes for Forge: the theme format, its validation and the icon resolver (pure, WebAssembly-friendly)."
order: 80
tags: [crate, rust]
---

# forge-icons

`crates/forge-icons` — File icon themes for Forge: the theme format, its validation and the icon resolver (pure, WebAssembly-friendly).

## Purpose

The model behind [icon themes](../icon-themes.md): which icon of a theme a file or folder
gets. Every file icon in the UI asks the resolver of the active theme first and falls back to
Forge's own icons without an answer.

| Item | Summary |
| --- | --- |
| `IconThemeDef` | `icon-theme.json`: `icons` (id → SVG path), defaults (`file`, `folder`, `folderExpanded`), `fileNames`, `fileExtensions`, `filePatterns`, `fileRegex`, `folderNames`, `folderNamesExpanded`, `options`; `parse`, `validate` (paths, references, regexes, options) |
| `IconThemeOptions`, `TreeArrows` | the look a theme asks for: tree arrows, accent color, monochrome, icon size |
| `Association`, `AssociationKind` | the user's own rules (`fileIcons.associations`): file name, extension, pattern, regex, folder name |
| `IconResolver` | `new(theme, associations)` (bad associations are skipped and reported), `resolve(path, IconKind)` with a cache per path and kind |
| `check_svg`, `is_svg_path`, `glob_match` | SVG limits and safety checks, package paths, globs (`*`, `**`, `?`, `[a-z]`, `[!x]`) |

Matching order for files: associations, file names (exact, then ignoring case), patterns,
regular expressions, extensions (longest compound first), the default icon. Folders:
associations, expanded or closed names, defaults.

## Used by

- `forge-plugins` (`icon_themes::load`, install checks) and through it the IPC command
  `icon_theme_load`
- `forge-wasm` (`icons_set_theme`, `icons_resolve`, `icons_validate_theme`): the UI resolves
  icons synchronously with it in the desktop app and in browser mode
- UI: `app/ui/src/features/iconThemes`, `app/ui/src/shell/FileIcon.tsx`

## Dependencies

- Workspace crates: none
- External: `serde`, `serde_json`, `regex-lite`

## Tests

Theme validation, file and folder matching order, user associations, skipped associations,
the per-path cache, globs and SVG checks.

```sh
cargo test -p forge-icons
```
