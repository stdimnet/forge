# Forge – notes for coding agents

Forge is a PHPStorm-style IDE: Rust + Tauri 2 backend, SolidJS + CodeMirror 6 UI.
Packs: PHPForge (PHP/Laravel/Symfony/Doctrine), WebForge, LuaForge, AIForge (AI agents).

## Layout

- `crates/forge-*`: Rust libraries (core, plugin-api, lsp, vcs, ai, agent-cli, ...)
- `packs/<pack>/<plugin>`: language and framework packs
- `app/src-tauri`: Tauri shell, IPC commands (`src/commands/<area>.rs`), plugin manifests (`plugins/*.toml`)
- `app/ui/src/features/<name>`: UI features; `app/ui/src/mock`: browser-mode backend
- `docs/ARCHITECTURE.md`: contribution points and IPC contracts. Read it before larger changes.

## Skills

Task recipes live in `.claude/skills/`:
`forge-dev-setup`, `add-tool-window`, `add-ipc-command`, `add-language-support`,
`add-plugin-or-pack`, `add-setting`, `add-keymap-action`, `verify-ui-with-screenshots`,
`quality-gates`, `build-release`.

## Rules

- Everything is written in English: code, comments, docs, UI text and commits.
- Follow PHPStorm's layout, wording, behaviour and default keymap; colors are Forge's own.
  Use only the CSS variables from `app/ui/src/theme/theme.css` and icons from `lucide-solid`.
- Shortcuts only through the keymap (`app/ui/src/keymap/`). Never add your own keydown listeners.
- Every IPC command needs a browser mock. Mocks load only in browser mode and must never write
  into real projects.
- Persistent options go to settings (plugin manifest schema), secrets to the OS keychain
  (`keyring`); never use `localStorage` for them.
- Keep `app/src-tauri` thin and put logic in tested library crates.
- Before committing, run the `quality-gates` skill. UI changes also need screenshots, and you must
  look at them.
- Commits: small, imperative subject lines, no co-author or other trailers. Branch names describe
  the work (`feat/…`, `fix/…`, `docs/…`).
- CI runs only on manual dispatch. Do not trigger GitHub workflows.
