---
title: Development setup
description: Install the toolchains, run Forge in browser mode or as a desktop app, use the demo projects and keep builds fast.
order: 30
tags: [setup, development, pnpm, tauri]
---

# Development setup

Recipe: [`.claude/skills/forge-dev-setup`](../../../.claude/skills/forge-dev-setup/SKILL.md).

## Toolchains

- **Rust** stable, pinned by `rust-toolchain.toml` (with rustfmt and clippy).
- **Node 22** and **pnpm 10**.
- **Linux**: the Tauri system libraries.

```sh
# Debian/Ubuntu
sudo apt-get install libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev libssl-dev build-essential
# Arch/EndeavourOS
sudo pacman -S --needed webkit2gtk-4.1 libayatana-appindicator librsvg xdotool openssl base-devel
```

macOS needs the Xcode command line tools, Windows the MSVC build tools and WebView2 (see
Tauri's prerequisites).

## Install and run

```sh
cd app/ui
pnpm install
pnpm dev          # browser mode: http://localhost:1420 (mocked backend, demo Laravel project)
pnpm tauri dev    # desktop app with the real Rust backend
```

| URL (browser mode) | Opens |
| --- | --- |
| `/` | the Laravel demo project `acme-shop` with `app/Models/User.php` |
| `/?project=symfony` | Symfony + Doctrine demo |
| `/?project=react`, `vue`, `angular` | WebForge demos |
| `/?project=lua` | the LÖVE demo game `pixel-dungeon` |
| `/?project=remote` | a remote project `deploy@shop.example.com:/var/www/shop` |
| `/?welcome` | the Welcome screen |
| `/?welcome=remote` | Welcome › Remote Development |
| `?windowControls=gnome` (also `windows`, `macos`, `kde`, `linux`) | force a title bar style |
| `/?splash` | the startup splash with the mocked loading steps, then the workbench |
| `/?splash&simulateUpdate=0.2.0` | the splash offering a simulated update (fake download, nothing installed) |
| `/?splash&splashPause=40` | the splash held at about 40 % (screenshots) |
| `/?simulateUpdate=0.2.0` | Help \| Check for Updates… offers a simulated update |

Desktop development builds never check for real updates. `FORGE_SIMULATE_UPDATE=0.2.0 pnpm
tauri dev` shows the update panel on the splash with a fake download instead; see
[Release builds](release-builds.md#updates-and-signing).

Browser mode is the fastest loop for UI work and screenshots. Use `pnpm tauri dev` to test the
real file system, git, language servers, PTYs, databases and Docker.

## Useful scripts (`app/ui`)

| Script | Does |
| --- | --- |
| `pnpm typecheck` | `tsc --noEmit` |
| `pnpm test` | vitest |
| `pnpm build` | typecheck + production build into `app/ui/dist` |
| `pnpm screenshots [scenario…]` | Playwright screenshots into `docs/screenshots/` |
| `pnpm wasm` | rebuild `crates/forge-wasm` into `src/wasm/pkg` |
| `pnpm agents [x86_64\|aarch64]` | build the static remote agents into `target/agents/` |
| `pnpm langservers install [--accept-intelephense-licence] [package…]` | pre-install npm language servers (and Node.js if missing) into Forge's data directory, like the IDE does; also `updates`, `where` |
| `pnpm release …` | agents + `tauri build` with bundled agents |
| `pnpm tauri …` | the Tauri CLI (runs from `app/` through `scripts/tauri.mjs`, which adds the updater signing settings to `tauri build`) |

## Keeping Rust builds manageable

- `export CARGO_PROFILE_DEV_DEBUG=0` saves a lot of disk space and link time.
- With several git worktrees, give each its own `CARGO_TARGET_DIR`; a shared target dir can
  return stale builds of workspace crates.
- Test one crate at a time while iterating: `cargo test -p forge-keymap`.

## Debugging

- Rust logs: `RUST_LOG=debug pnpm tauri dev` (or a filter such as `RUST_LOG=forge_lsp=trace`).
- The WebView's developer tools open with the usual shortcut in debug builds (right-click →
  Inspect on Linux).
- In browser mode, commands can be run from the console: `window.__forgeRunCommand("settings.open")`.
