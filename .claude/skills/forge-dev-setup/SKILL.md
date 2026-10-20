---
name: forge-dev-setup
description: Set up a Forge development environment and run the IDE. Use when starting work on this repo, when a build fails because tools or system libraries are missing, or when asked how to run Forge in the browser or as a desktop app.
---

# Forge development setup

Forge is a Rust + Tauri 2 desktop app with a SolidJS UI in `app/ui`.

## 1. Toolchains

- Rust stable (pinned via `rust-toolchain.toml`, includes rustfmt + clippy).
- Node 22 and pnpm 10.
- Linux only, Tauri system libraries:
  - Debian/Ubuntu: `sudo apt-get install libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev libssl-dev build-essential`
  - Arch/EndeavourOS: `sudo pacman -S --needed webkit2gtk-4.1 libayatana-appindicator librsvg xdotool openssl base-devel`

## 2. Install

```sh
cd app/ui
pnpm install
```

## 3. Run

| Goal | Command (from `app/ui`) |
| --- | --- |
| UI only in the browser, mocked backend with a demo Laravel project | `pnpm dev` → http://localhost:1420 |
| Welcome screen in browser mode | http://localhost:1420/?welcome |
| Desktop app with the real Rust backend | `pnpm tauri dev` |

Browser mode is the fastest loop: every IPC command has a mock in `app/ui/src/mock/`.
Use it for UI work and screenshots; use `pnpm tauri dev` to test real file system, git, LSP, PTY.

## 4. Where things live

- `crates/forge-*` – Rust libraries (core, plugin API, lsp, vcs, ai, agent-cli, ...)
- `packs/<pack>/<plugin>` – language/framework packs (PHPForge, ...)
- `app/src-tauri` – Tauri shell, IPC commands (`src/commands/<area>.rs`), built-in plugin manifests (`plugins/*.toml`)
- `app/ui/src/features/<name>` – UI features; `app/ui/src/mock` – browser-mode backend
- `docs/ARCHITECTURE.md` – contribution points and IPC contracts

Next steps: see the `add-*` skills, then `quality-gates` before committing.
