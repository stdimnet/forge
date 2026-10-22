---
name: add-keymap-action
description: Bind a keyboard shortcut or add an action to Forge's PHPStorm-compatible keymap (Find Action, menus, Keymap settings). Use whenever a feature needs a shortcut like Alt+Insert, Ctrl+K or Alt+7, or when a shortcut does nothing or fires twice.
---

# Add a keymap action

All shortcuts go through the keymap engine in Rust (`crates/forge-keymap`); `app/ui/src/keymap/`
only captures key events and runs the resolved action. It uses PHPStorm's default keymaps
(Windows/Linux and macOS) keyed by **PHPStorm action ids**, which are also Forge command ids.

**Never** add your own `window.addEventListener("keydown", ...)` for shortcuts. It breaks user
overrides, focus contexts and conflict detection, and causes double toggles.

## Implement an existing PHPStorm action

Most actions already exist in `crates/forge-keymap/data/phpstorm.json` with the correct default
keys. Just register a command with that id:

```ts
registerCommand({ id: "ActivateStructureToolWindow", title: "Structure", run: () => toggleToolWindow("structure") });
```

If your feature already uses another id (e.g. `php.generate`), alias it in `ACTION_ALIASES`
(`crates/forge-keymap/src/catalog.rs`):

```rust
("Generate", &["php.generate"]),
```

After changing the Rust keymap, rebuild the browser-mode bundle: `pnpm --dir app/ui wasm`.

## Add a new action

For plugin-specific actions with default shortcuts:

```ts
registerKeymapActions([
  { id: "CommandLineToolsAction", title: "Run Command…", group: "Tools",
    shortcuts: { $default: ["Ctrl+Shift+X"], macOS: ["Cmd+Shift+X"] } },
]);
registerCommand({ id: "CommandLineToolsAction", title: "Run Command…", run: () => openRunCommand() });
```

Shortcut syntax: `Ctrl+Shift+A`, `Alt+Insert`, chords `Ctrl+K Ctrl+C`, double taps `Shift Shift`,
macOS `Cmd`/`Opt`. Physical key codes are used, so keyboard layouts work.

## Put it in the main menu

The main menu (☰, `Alt+\`) is PHPStorm's structure in `crates/forge-keymap/src/menu.rs`; an
action listed there is enabled as soon as a command implements it. Plugins add entries in their
manifest or from the UI:

```toml
[[plugin.contributions.mainMenu]]
path = "Tools/_Laravel"          # "_" marks the mnemonic; missing submenus are created
items = [{ type = "action", id = "Laravel.RunArtisan", label = "Run _Artisan Command..." }]
```

```ts
registerMainMenuEntries("Run", [action("PhpListenDebugAction")], { after: "XDebugger.MuteBreakpoints" });
registerMainMenuToggle("PhpListenDebugAction", listening); // check mark for checkable entries
```

## Check

- `cargo test -p forge-keymap`: the keymap tests detect unexpected conflicts.
- Find Action (Ctrl+Shift+A) lists the action with its shortcut.
- Settings → Keymap shows it and lets users rebind it.
