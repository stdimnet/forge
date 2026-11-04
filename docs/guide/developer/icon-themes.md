---
title: Icon themes
description: The icon theme contribution point - icon-theme.json, matching rules, the Rust resolver (forge-icons), registerIconTheme, plugin iconThemes contributions with SVGs, look hooks (tree arrows, accent color, monochrome, size), user associations and the Shapes Icons example.
order: 62
tags: [ui, icons, icon theme, plugins, extension points]
---

# Icon themes

An icon theme replaces the file and folder icons in every place that shows them: the Project
view, editor tabs, Recent Files, Search Everywhere, Find in Files, the Structure tool window,
code completion and the navigation bar. The user picks the active theme in
Settings | Appearance & Behavior | File Icons (`fileIcons.theme`). With no theme, or for a file
the theme has no icon for, Forge shows its own icons (`app/ui/src/shell/FileIcon.tsx`).

A theme is **data only**: a JSON file that maps names to icon ids and an SVG per icon. It runs
no code and needs no permission.

## Where the parts live

| Part | Where |
| --- | --- |
| Theme format, validation, SVG checks, resolver with a per-path cache | `crates/forge-icons` ([crate page](crates/forge-icons.md)) |
| Resolver in the UI (synchronous, desktop and browser) | `crates/forge-wasm/src/icons.rs` (`icons_set_theme`, `icons_resolve`) |
| Plugin contribution, loading a theme from a package | `crates/forge-plugins` (`contributes.iconThemes`, `icon_themes::load`) |
| IPC | `icon_theme_load({ id }) -> LoadedIconTheme` (`commands/third_party.rs`), browser mock in `mock/pluginsMock.ts` |
| Contribution point, active theme, look hooks | `app/ui/src/features/iconThemes/state.ts` (`registerIconTheme`, `themedIcon`, `iconLook`) |
| Settings page and associations editor | `app/ui/src/features/iconThemes/index.tsx` |
| Tree arrows | `app/ui/src/shell/TreeArrow.tsx` |
| Example | `sdk/examples/shapes-icons` |

## `icon-theme.json`

```json
{
  "icons": { "file": "icons/file.svg", "php": "icons/php.svg", "folder": "icons/folder.svg", "folder-open": "icons/folder-open.svg" },
  "file": "file",
  "folder": "folder",
  "folderExpanded": "folder-open",
  "fileNames": { "composer.json": "composer" },
  "fileExtensions": { "php": "php", "blade.php": "blade" },
  "filePatterns": [{ "pattern": "*Test.php", "icon": "test" }, { "pattern": "**/tests/*.php", "icon": "test" }],
  "fileRegex": [{ "pattern": "^\\.env(\\..+)?$", "icon": "env" }],
  "folderNames": { "src": "folder-src" },
  "folderNamesExpanded": { "src": "folder-src-open" },
  "options": { "treeArrows": "triangle", "accentColor": "#e0a040", "monochrome": false, "iconSize": 16 }
}
```

| Key | Meaning |
| --- | --- |
| `icons` | icon id → SVG file, relative to the theme file (inside the package, `.svg`) |
| `file`, `folder`, `folderExpanded` | default icons; without `file` unmatched files keep Forge's icon |
| `fileNames` | exact file names (then ignoring case) |
| `filePatterns` | globs, in order: `*`, `**`, `?`, `[a-z]`, `[!x]`; a pattern with `/` matches the whole path |
| `fileRegex` | regular expressions on the file name, in order |
| `fileExtensions` | extensions without the dot; the longest compound one wins (`blade.php` before `php`) |
| `folderNames`, `folderNamesExpanded` | folders by name; an expanded folder falls back to its closed icon |
| `options` | the theme's look: `treeArrows` (`chevron`, `triangle`, `none`), `accentColor` (`#rgb` / `#rrggbb`, folders), `monochrome`, `iconSize` (8–32 px) |

Matching order for a file: the user's associations, file names, patterns, regular expressions,
extensions, the default `file`. For a folder: the user's folder associations, the expanded or
closed folder names, the defaults. Rules that name an icon the theme does not declare are
reported by validation, and the resolver never answers with such an id.

Limits and checks (`forge_icons::check_svg`): at most 5,000 icons, 128 KiB per SVG, an `<svg>`
root, no scripts, event handler attributes, `foreignObject`, `iframe`, `javascript:` URLs or
external references. Forge draws theme icons as CSS images (or masks), which cannot run
scripts; the checks keep packages honest anyway.

## Contributing a theme from a plugin

In `forge-plugin.json`:

```json
"contributes": {
  "iconThemes": [
    { "id": "acme.material-icons.dark", "label": "Material (dark)", "path": "theme/icon-theme.json" }
  ]
}
```

- `id` starts with the plugin id (like every contribution) and is the value of
  `fileIcons.theme`.
- `path` is a `.json` file inside the package. Put it in a folder of its own next to its SVGs:
  `pnpm plugin pack` packs that whole folder. A plugin without `main` (no code) is fine.
- The theme and every SVG it names are loaded and checked when the package is installed, so a
  broken theme is refused with the field that is wrong.
- No permission is needed and the theme loads in Safe Mode: Forge reads only the theme file
  and `.svg` files of the plugin's own package (`icon_themes::theme_file_allowed`).

When the plugin is enabled, the plugin host registers its themes with `registerIconTheme`;
disabling or uninstalling it removes them, and the icons fall back to Forge's.

## `registerIconTheme` (built-in features)

```ts
import { registerIconTheme } from "@/features/iconThemes/state";

const remove = registerIconTheme({
  id: "forge.example.theme",
  label: "Example",
  source: "Example plugin",           // shown next to the label in the picker
  load: async () => ({ definition, svgs }), // IconThemeDef + icon id → SVG text
});
```

The active theme is loaded when it is selected, validated and handed to the Rust resolver
together with the user's associations (`fileIcons.associations`, kinds `fileName`,
`extension`, `pattern`, `regex`, `folderName`). Problems (an association naming a missing
icon, a regex that does not compile) are shown on the settings page; the offending rules are
skipped.

## Showing icons

`<FileIcon name path? dir? open? size? dimmed? />` asks the active theme first. Pass `path`
when it is known so path patterns apply. Outside Solid (CodeMirror), use
`themedIconElement(path, kind, size)`; it returns `null` when the theme has no icon. Both use
`iconStyle()`, which applies the look hooks.

## Look hooks

`iconLook()` combines the theme's `options` with the user's settings, which win:

| Hook | Setting | Effect |
| --- | --- | --- |
| Tree arrows | `fileIcons.treeArrows` (`theme`, `chevron`, `triangle`, `none`) | `<TreeArrow open />` in the Project view and Structure |
| Monochrome | `fileIcons.monochrome` (`theme`, `on`, `off`) | icons drawn as a mask in `--fg-muted` (folders in the accent color) |
| Accent color | `fileIcons.accentColor` | folder icons drawn as a mask in that color |
| Icon size | `fileIcons.size` (0 = theme / view default) | width and height of themed icons |

## The Shapes Icons example

`sdk/examples/shapes-icons` is a complete icon theme plugin with 25 SVGs of simple shapes
(MIT, drawn for Forge): PHP, Blade, JS/TS, JSON, Markdown, CSS, HTML, YAML, `.env`, Git files,
lock files, `composer.json`, tests, and `src`, `app`, `tests`, `config` and `public` folders.

```sh
cd sdk/examples/shapes-icons && node ../../scripts/forge-plugin.mjs pack .
```

Install the `.forgeplugin` with Settings | Plugins | Install Plugin from Disk, then choose
"Shapes (Shapes Icons)" in Settings | Appearance & Behavior | File Icons. Browser mode has it
installed already. `cargo test -p forge-plugins` loads the example from disk and checks a few
resolutions.
