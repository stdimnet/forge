# AGENTS.md – Forge

Instructions for coding agents (Claude Code, Codex, Gemini CLI, Cursor, …) and humans working
on this repository. Read this file completely before changing code.

## What Forge is

Forge is a PHPStorm-style IDE for Windows, Linux and macOS.

- **Backend:** Rust + Tauri 2 (memory safe, one native binary).
- **UI:** SolidJS + TypeScript + CodeMirror 6, icons from `lucide-solid`.
- **Packs:** PHPForge (PHP, Laravel, Symfony, Doctrine), WebForge (HTML/CSS/JS/TS, Vue, React,
  Angular), LuaForge (Lua). AI (providers, Claude Code / Gemini CLI chat, AI commit messages)
  is part of the core.
- Layout, wording, behaviour and the default keymap follow PHPStorm 1:1; the colors are Forge's
  own ("Ember" dark / "Anvil" light).
- Repository: https://github.com/Reeperk/Forge (default branch `main`).

## Repository layout

```
crates/forge-*          Rust libraries: core (workspace, settings, search), plugin-api, lsp,
                        vcs, ai, agent-cli, project (type detection, generators), …
packs/<pack>/<plugin>   Language and framework packs, e.g. packs/phpforge/lang-php
app/src-tauri/          Tauri shell: IPC commands (src/commands/<area>.rs, registered in
                        src/lib.rs) and built-in plugin manifests (plugins/*.toml)
app/ui/                 SolidJS UI
  src/features/<name>/  UI features (tool windows, dialogs, editor extensions)
  src/editor/           CodeMirror setup, languages, template data languages, theme
  src/keymap/           key capture and dispatch (the engine is crates/forge-keymap)
  src/ipc/              typed clients for IPC commands
  src/mock/             browser-mode backend (mocks + demo projects)
  scripts/scenarios/    Playwright screenshot scenarios
docs/                   ARCHITECTURE.md, ROADMAP.md, screenshots/
.claude/skills/         step-by-step recipes for common tasks (see below)
```

`docs/ARCHITECTURE.md` documents every contribution point and the shared IPC contracts. Read it
before larger changes.

## Running Forge

```sh
cd app/ui && pnpm install
pnpm dev          # browser mode with mocked backend + demo Laravel project (http://localhost:1420)
pnpm tauri dev    # desktop app with the real Rust backend
pnpm tauri build --bundles appimage   # release build
```

Linux needs the Tauri system libraries (webkit2gtk-4.1 etc.), see `.claude/skills/forge-dev-setup`.

## Skills (task recipes)

Use the matching skill in `.claude/skills/<name>/SKILL.md` before starting a task:

| Skill | Use it to |
| --- | --- |
| `forge-dev-setup` | install toolchains and run Forge (browser / desktop) |
| `add-tool-window` | add a panel or replace a placeholder tool window |
| `add-ipc-command` | add a Rust command + typed client + browser mock |
| `add-language-support` | add syntax highlighting, file types and template data languages |
| `add-plugin-or-pack` | create a plugin or pack (manifest, crate, settings, UI) |
| `add-setting` | add settings and Settings dialog pages |
| `add-keymap-action` | bind shortcuts / actions through the PHPStorm keymap |
| `verify-ui-with-screenshots` | capture and review UI screenshots |
| `quality-gates` | run all checks before committing |
| `build-release` | build and smoke-test an AppImage / installer |

## Rules

### Language and style
- Everything is written in **English**: code, comments, docs, UI text and commit messages.
- Match the surrounding code: naming, comment density and idioms.
- UI: PHPStorm New UI layout and wording. Use only the CSS variables from
  `app/ui/src/theme/theme.css`, both dark and light, and icons from `lucide-solid`.

### Architecture
- **Rust computes, TypeScript renders.** Logic, data models and parsers belong in Rust crates with
  unit tests; the UI renders and forwards events. Only per-keystroke editor work (CodeMirror
  tokenizers, decorations) stays in TypeScript. Keep `app/src-tauri` thin.
- Browser mode reuses pure Rust crates through WebAssembly (`crates/forge-wasm`, `@/wasm`)
  instead of duplicating logic in mocks.
- Features plug in through contribution points: `registerToolWindow`, `registerCommand`,
  `registerSettingsPage`, `registerStatusItem`, `registerEditorExtension`,
  `registerSearchProvider`, `registerLanguage`, `registerKeymapActions`.
- **Shortcuts only through the keymap** (`crates/forge-keymap`, UI in `app/ui/src/keymap/`).
  Use PHPStorm action ids or `ACTION_ALIASES`. Never add your own `keydown` listeners for
  shortcuts.
- **Every IPC command needs a browser mock** in `app/ui/src/mock/`. Mocks load only in browser
  mode and must never write into real projects.
- Persistent options go into **settings**: a schema in the plugin manifests, read with
  `getSetting`. Secrets go into the **OS keychain** via `keyring`. Never use `localStorage` for
  either.
- Caches belong in the user cache dir, never inside the user's project.
- After changing plugin manifests, regenerate the browser snapshot with
  `FORGE_UPDATE_SNAPSHOTS=1 cargo test -p forge-app`.
- Plugins activate only when the project uses them (`app/ui/src/state/project.ts`:
  `usesFramework`, `isPluginActive`).
- Language servers are external and user-installed, e.g. Intelephense free, or premium with
  the user's own licence key entered in Forge. Never bundle paid tools or keys.

### Quality
- Before every commit, run the `quality-gates` skill:
  `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace`, `pnpm typecheck`, `pnpm test`, `pnpm build`.
- UI changes need screenshots (`verify-ui-with-screenshots`), and you must look at them.
- Tests must not need network access, installed language servers or real code-host accounts.
  Use fakes and local mock servers.
- With several git worktrees, give each its own `CARGO_TARGET_DIR`. Set
  `CARGO_PROFILE_DEV_DEBUG=0` to save disk space.

### Git
- Branch names describe the work: `feat/…`, `fix/…`, `docs/…`, `chore/…`.
- Small commits with imperative English subjects ("Add routes tool window").
- **No co-author or other trailers** in commits. Do not mention AI tools in commit messages.
- Commit early and often during long tasks, so work is not lost.
- CI (`.github/workflows/ci.yml`) runs **only on manual dispatch**. Never trigger GitHub
  workflows. Every branch must keep the `workflow_dispatch`-only trigger.
- Do not push or open pull requests unless you were asked to.
