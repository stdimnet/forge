---
title: Keymap actions
description: How the Rust keymap engine works, implementing PHPStorm actions, aliases, plugin actions with default shortcuts, focus contexts and conflict tests.
order: 110
tags: [keymap, shortcuts, actions]
---

# Keymap actions

Recipe: [`add-keymap-action`](../../../.claude/skills/add-keymap-action/SKILL.md). Every
shortcut goes through the keymap engine. **Never** add a `keydown` listener for a shortcut: it
breaks user overrides, focus contexts and conflict detection, and causes double toggles.

## The engine

`crates/forge-keymap` (pure Rust):

- `shortcut`: parsing (`Ctrl+K Ctrl+C`, `Shift Shift`, `Cmd+,`), canonical ids, display strings
  (`Ctrl+Shift+A` / `⇧⌘A`) and physical key names of DOM events;
- `catalog`: PHPStorm's default keymaps from `data/phpstorm.json` (id, title, group,
  Windows/Linux and macOS shortcuts, `native` flag) and `ACTION_ALIASES`;
- `engine`: bindings, conflicts and the `Resolver` state machine (chords, double taps, focus
  contexts `editor`, `input`, `toolwindow`, `global`, `terminal`);
- `keymap`: `KeymapService` combining the catalog, plugin actions, registered commands and user
  overrides from the `keymap.*` settings.

The UI (`app/ui/src/keymap/`) captures key events, classifies the focus context and asks the
resolver — synchronously, through the WebAssembly build, in desktop and browser mode alike —
then runs the returned action with `runAction`. Unimplemented actions show "not available yet".
IPC: `keymap_actions`, `keymap_register_actions`, `keymap_set_override`, `keymap_conflicts`,
`keymap_find_by_shortcut`, `keymap_resolve`, `keymap_import_legacy`.

## Implementing a PHPStorm action

Most actions already exist with the right default keys. Register a command with that id:

```ts
registerCommand({ id: "ActivateStructureToolWindow", title: "Structure",
  run: () => toggleToolWindow("left", "structure") });
```

If your feature already uses another id, alias it in `ACTION_ALIASES`
(`crates/forge-keymap/src/catalog.rs`):

```rust
("Generate", &["php.generate"]),
```

Rebuild the browser bundle after changing Rust keymap code: `pnpm --dir app/ui wasm`.

## Plugin actions

```ts
registerKeymapActions([
  { id: "Laravel.RunArtisan", title: "Run Artisan Command…", group: "Laravel",
    shortcuts: { $default: ["Ctrl+Shift+X"], macOS: ["Cmd+Shift+X"] } },
  { id: "Console.Jdbc.Execute", title: "Execute", group: "Database", context: "editor",
    shortcuts: { $default: ["Ctrl+Enter"], macOS: ["Cmd+Enter"] } },
]);
registerCommand({ id: "Laravel.RunArtisan", title: "Run Artisan Command…", run: openArtisanPopup });
```

- `group` places the action in the Keymap tree (`Plugins/<Name>` by convention, or a
  `Main Menu/...` group).
- `context: "editor" | "toolwindow"` limits where the shortcut applies, so the same keys can
  mean different things (`Alt+Insert` adds a row in a data grid, opens Generate in the editor).
- Shortcut syntax: `Ctrl+Shift+A`, `Alt+Insert`, chords `Ctrl+K Ctrl+C`, double taps
  `Shift Shift`, macOS `Cmd`/`Opt`. Physical key codes are used, so layouts work.

Manifests can also declare `[[contributions.keybindings]]` (`command`, `key`, optional `keymap`
and `when`).

> **Note:** some editor-local keys are bound with CodeMirror keymaps inside a feature's editor
> extension (for example `Ctrl+O`/`Ctrl+I` in PHP files, `Mod-b`, `Mod-p`, `Mod-Alt-l`). Prefer
> the keymap engine for new actions.

## Checks

- `cargo test -p forge-keymap`: the tests detect unexpected conflicts in the default keymaps.
- Find Action lists the action with its shortcut; Settings | Keymap shows it and lets users
  rebind it.
- Update [the keymap reference](../reference/keymap.md).
