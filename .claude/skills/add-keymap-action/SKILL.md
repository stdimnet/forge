---
name: add-keymap-action
description: Bind a keyboard shortcut or add an action to Forge's PHPStorm-compatible keymap (Find Action, menus, Keymap settings). Use whenever a feature needs a shortcut like Alt+Insert, Ctrl+K or Alt+7, or when a shortcut does nothing or fires twice.
---

# Add a keymap action

All shortcuts go through the keymap engine in `app/ui/src/keymap/`. It uses PHPStorm's default
keymaps (Windows/Linux and macOS) keyed by **PHPStorm action ids**, which are also Forge
command ids.

**Never** add your own `window.addEventListener("keydown", ...)` for shortcuts. It breaks user
overrides, focus contexts and conflict detection, and causes double toggles.

## Implement an existing PHPStorm action

Most actions already exist in `app/ui/src/keymap/phpstormKeymap.ts` with the correct default
keys. Just register a command with that id:

```ts
registerCommand({ id: "ActivateStructureToolWindow", title: "Structure", run: () => toggleToolWindow("left", "structure") });
```

If your feature already uses another id (e.g. `php.generate`), alias it in `ACTION_ALIASES`:

```ts
Generate: ["php.generate"],
```

## Add a new action

For plugin-specific actions with default shortcuts:

```ts
registerKeymapActions([
  { id: "Laravel.RunArtisan", title: "Run Artisan Command…", group: "Laravel",
    shortcuts: { $default: ["Ctrl+Shift+X"], macOS: ["Cmd+Shift+X"] } },
]);
registerCommand({ id: "Laravel.RunArtisan", title: "Run Artisan Command…", run: openArtisanPopup });
```

Shortcut syntax: `Ctrl+Shift+A`, `Alt+Insert`, chords `Ctrl+K Ctrl+C`, double taps `Shift Shift`,
macOS `Cmd`/`Opt`. Physical key codes are used, so keyboard layouts work.

## Check

- `pnpm test`: keymap tests detect unexpected conflicts.
- Find Action (Ctrl+Shift+A) lists the action with its shortcut.
- Settings → Keymap shows it and lets users rebind it.
