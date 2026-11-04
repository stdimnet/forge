---
title: Localization
description: How Forge's UI is translated - forge-i18n, ICU MessageFormat catalogs, key naming, t() and <T> in the UI, Rust-side text, plugin manifests, pnpm i18n:check and adding a language.
order: 105
tags: [i18n, localization, translation, catalogs, messageformat]
---

# Localization

Forge's UI text goes through message catalogs. English is the source language; German is the
first translation. The logic (parsing, lookup, fallback, plurals, number and date formats)
lives in the Rust crate [forge-i18n](crates/forge-i18n.md); the UI calls it synchronously
through WebAssembly, in the desktop app and in browser mode alike.

## Catalogs

Catalogs are flat JSON files `crates/forge-i18n/locales/<locale>/<area>.json`, one object of
`key → message` per file, compiled into the binary and the WebAssembly bundle:

```json
{
  "vcs.commit": "Commit",
  "vcs.files": "{count, plural, one {# file} other {# files}}",
  "find.summary": "{count}{plus} {count, plural, one {match} other {matches}} in {files}{plus} {files, plural, one {file} other {files}} in {duration}"
}
```

| Area | Contents |
| --- | --- |
| `common` | buttons and words many places share (`common.ok`, `common.cancel`, `common.apply`) |
| `shell` | main toolbar, status bar, tool window chrome, editor tabs and menus, notifications, Problems |
| `settings` | the Settings dialog, page titles (`settings.page.<id>`), groups, Plugins page |
| `search`, `vcs`, `run`, `dialogs` (Project view, Rename, New Project, …), `welcome`, `errors` | their features |
| `menu`, `actions`, `plugins` | **generated in English** (see below), translated by hand |

A new area file must also be listed in `BUILTIN` in `crates/forge-i18n/src/catalog.rs`; a test
fails when a file on disk is missing there.

### Messages

Messages use [ICU MessageFormat](https://unicode-org.github.io/icu/userguide/format_parse/messages/):

| Syntax | Example |
| --- | --- |
| argument | `Open {name}?` |
| plural (`zero one two few many other`, `=n`, `#`, `offset:`) | `{count, plural, =0 {No files} one {# file} other {# files}}` |
| ordinal | `{n, selectordinal, one {#st} two {#nd} few {#rd} other {#th}}` |
| select | `{kind, select, file {File} other {Folder}}` |
| number (`integer`, `percent`) | `{size, number}` → *1,234.5* / *1.234,5* |
| date / time (`short`, `medium`, `long`), milliseconds since the epoch | `{when, date, medium}` → *Nov 4, 2026* / *04.11.2026* |

An apostrophe before a brace quotes it: write `Run ''{name}''` for *Run 'app'*. A lone
apostrophe elsewhere (`don't`) needs no escaping. The key-parity test reports a translation
whose arguments differ from the English message, which also catches a forgotten `''`.

### Key naming

Keys are dotted, lower camel case segments, starting with the area or feature:
`toolwindow.project.title`, `vcs.commitOptions`, `settings.page.editor.general`,
`statusbar.indent.tooltip`. Keys derived from ids keep the id as it is:

| Text | Key | English source |
| --- | --- | --- |
| main menu submenu | `menu.group.<slug>` (`_Recent Projects` → `recent-projects`) | `crates/forge-keymap/src/menu.rs` |
| own label of a menu action | `menu.<action id>` | `menu.rs` |
| action title (Find Action, keymap, menu) | `action.<action id>` | `crates/forge-keymap/data/phpstorm.json`, manifest commands |
| plugin name / description | `plugin.<id>.name`, `plugin.<id>.description` | manifests |
| tool window title | `toolwindow.<id>.title` | manifests or `registerToolWindow` |
| setting title / description / option / section | `setting.<key>.title`, `.description`, `.option.<value>`, `setting.section.<page>.<slug>` | manifests |
| run configuration type | `runconfig.<id>.name`, `.description`, `.field.<key>` | manifests |
| Settings page / group | `settings.page.<page id>`, `settings.group.<slug>` | `registerSettingsPage` |

`locales/en/menu.json`, `actions.json` and `plugins.json` are generated from the code that owns
the English text by snapshot tests; never edit them. After changing `menu.rs`, the PHPStorm
action catalog or a manifest, run:

```sh
FORGE_UPDATE_SNAPSHOTS=1 cargo test -p forge-keymap -p forge-app
```

## Using translations in the UI

```ts
import { T, t, tOr } from "@/i18n";

t("vcs.commit");                                  // "Commit" / "Committen"
t("vcs.files", { count: n });                     // "3 files" / "3 Dateien"
t("toolbar.settings", { shortcut: "Ctrl+Alt+S" }); // shortcuts stay arguments
tOr(`search.tab.${id}`, fallback);                // a key built from data that may not exist
<T k="common.apply" />                            // reactive JSX text
```

- Add the English message to the right `locales/en/<area>.json` and the German one to
  `locales/de/<area>.json` in the same change. Hand-written areas must be complete in German
  (a test enforces it).
- `t()` reads the current language from a signal, so text rendered in JSX or a memo follows a
  language change at once. Text computed once (titles passed to `registerToolWindow`,
  `registerSettingsPage` or `registerContextMenu` at startup) is translated with the language
  of the session; changing the language offers a restart for that reason.
- Never build sentences from pieces (`` `${n} file${n === 1 ? "" : "s"}` ``): use one message with
  arguments and a plural. Keep keyboard shortcuts, file names and code as arguments.
- Do not translate command titles passed to `registerCommand` or `registerKeymapActions`: they
  stay English, and the keymap (Rust) shows the `action.<id>` translation with the English
  title as `titleEn`, which Find Action also matches.
- A local variable called `t` (common for "tab" or "type") shadows the import; rename the
  variable, or import `t as tr` as `features/run/ConfigurationsDialog.tsx` does.
- In vitest, `t()` loads the WebAssembly module synchronously on first use, so components
  render English text in tests without setup.

## Rust-side text

Text that Rust produces for the user goes through the process-wide localizer, whose language
the desktop app sets from `ide.language` at startup and whenever the setting changes (the
UI does the same for its WebAssembly instance):

```rust
forge_i18n::t("errors.safeMode.refused");
forge_i18n::t_args("errors.safeMode.refused", &forge_i18n::args! { "what" => what });
forge_i18n::t_or("menu.SaveAll", "Save _All"); // translation, or the English text from code
```

The main menu and action titles (`forge-keymap`), plugin manifests
(`forge_plugin_api::l10n::localize_plugin` / `localize_settings`, used by `plugins_list` and
`settings_schema`), Settings group titles (`settings_model`) and Safe Mode refusals
(`forge-trust`) are localized this way. Tests that need a language use their own
`Localizer` (`Localizer::new("de")`, or `Localizer::empty` plus `add_messages`) instead of the
process-wide one, so parallel tests do not interfere.

## Plugin manifests

Built-in manifests need no keys: their English text is the source and the keys above are
derived from ids. Third-party plugins can ship translations in their manifest:

```toml
[l10n.de]
"plugin.acme.tools.name" = "Acme-Werkzeuge"
"setting.acme.mode.title" = "Modus"
"setting.acme.mode.option.fast" = "Schnell"
```

The bundled catalogs win over a manifest's own table; locales follow the same fallback chain
(`de-AT` → `de` → English).

## Checking coverage

```sh
cd app/ui && pnpm i18n:check            # summary
pnpm i18n:check --list src/features/git  # every hard-coded string below a path
pnpm i18n:check --missing-de             # English keys without a German translation
```

The check fails (exit code 1) only when the code uses a key that the English catalog does not
have. It also reports German coverage per area and finds hard-coded user-visible strings in
`.tsx` files heuristically (JSX text, `title`, `placeholder`, `aria-label`, `label`, balloon
calls); `scripts/i18n-allowlist.json` lists strings that need no translation (product names,
units). Legacy strings do not fail the quality gates yet.

The Rust tests in `forge-i18n` are the hard gate for the catalogs: every file parses, no key is
defined twice, every German key exists in English with the same arguments (key parity), and
the hand-written areas are fully translated.

## Current coverage

`pnpm i18n:check` at the time German was added:

| Measure | Value |
| --- | --- |
| English catalog keys | 3,740 (of which 3,094 generated: 320 menu labels, 375 action titles, 2,399 manifest texts) |
| German translations | 1,711 (45.7 %): every hand-written key, all menu labels and PHPStorm action titles, and 370 of 2,399 manifest texts |
| Keys used in the UI code | 579 literal keys plus 9 dynamic `t()` calls |
| Hard-coded strings left in `.tsx` (heuristic) | 4,423 |

Translated: the main menu, Find Action and the keymap's PHPStorm actions, tool window titles
and chrome, the status bar file widgets, index and language server status, the Settings
dialog (tree, page titles, buttons, search) with the Appearance, System Settings, Language,
Editor General / Tabs / Font, Terminal, Git and Commit settings, the Plugins page, the
Welcome screen, Search Everywhere, Find in Files, Rename, New Project, Run/Debug
Configurations (dialog chrome) and the run widget, the Commit tool window, the Project view
and editor tab and editor context menus, notifications, the language restart prompt and Safe
Mode refusals.

Still English: the other tool windows and dialogs (Database 508 strings, Git log and dialogs
262, .NET 192, Debugger 185, code intelligence dialogs 155, C/C++ 147, code hosts 136, Code
With Me 118, …), most settings of the language packs (2,029 manifest texts), run
configuration fields and Run on targets, file templates and New Project generator names,
menu entries contributed by UI features (`registerMainMenuEntries` labels) and Forge's own
commands without a PHPStorm action id, Rust errors other than Safe Mode, the splash screen.

## Adding a language

1. Add the locale to `locales()` in `crates/forge-i18n/src/catalog.rs` with its own name
   (`Français`), create `locales/<id>/` with the area files and list them in `BUILTIN`.
2. Check the plural rules in `plural.rs` and the separators and date patterns in `format.rs`
   for the language; add them when missing.
3. Add the option to `ide.language` in `app/src-tauri/plugins/core.toml` and regenerate the
   snapshots (`FORGE_UPDATE_SNAPSHOTS=1 cargo test -p forge-app`), then `pnpm wasm`.
4. Translate, run `cargo test -p forge-i18n` and `pnpm i18n:check`, and take screenshots of
   the main window, Settings, Find Action and the Commit window in the new language to check
   for truncated text (`app/ui/scripts/scenarios/localization.mjs` shows how).
