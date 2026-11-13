# Contributing to Forge

Thanks for helping build Forge! This guide gets you from clone to first pull request.

## Quick start

```sh
cd app/ui
pnpm install
pnpm dev          # UI in the browser with a mocked demo project: http://localhost:1420
pnpm tauri dev    # desktop app with the real Rust backend
```

System requirements and details: [.claude/skills/forge-dev-setup](.claude/skills/forge-dev-setup/SKILL.md).

## How do I…

| Task | Guide |
| --- | --- |
| add a panel / tool window | [add-tool-window](.claude/skills/add-tool-window/SKILL.md) |
| call Rust from the UI | [add-ipc-command](.claude/skills/add-ipc-command/SKILL.md) |
| add syntax highlighting for a file type | [add-language-support](.claude/skills/add-language-support/SKILL.md) |
| create a plugin or language pack | [add-plugin-or-pack](.claude/skills/add-plugin-or-pack/SKILL.md) |
| add an option to Settings | [add-setting](.claude/skills/add-setting/SKILL.md) |
| add a shortcut / action | [add-keymap-action](.claude/skills/add-keymap-action/SKILL.md) |
| check my UI change | [verify-ui-with-screenshots](.claude/skills/verify-ui-with-screenshots/SKILL.md) |
| run all checks | [quality-gates](.claude/skills/quality-gates/SKILL.md) |
| build an AppImage / installer | [build-release](.claude/skills/build-release/SKILL.md) |

These guides are also [Claude Code skills](https://code.claude.com/docs/en/skills), so a
coding agent in this repo picks them up automatically. Rules for coding agents (and a compact
overview for humans) are in [AGENTS.md](AGENTS.md).

## Ground rules

- English everywhere: code, comments, docs, UI, commits.
- Look and feel like PHPStorm (layout, wording, shortcuts) with Forge's own colors.
- Shortcuts go through the keymap, settings through the settings schema, secrets into the OS keychain.
- Every backend call has a browser mock, so the whole UI runs in `pnpm dev`.
- Small commits with imperative subjects; descriptive branch names (`feat/…`, `fix/…`, `docs/…`).
- Run the [quality gates](.claude/skills/quality-gates/SKILL.md) before opening a pull request,
  and attach screenshots for UI changes.
- Every user-visible change updates the matching page of the user and developer guide in
  [docs/guide/](docs/guide/index.md) (structure and page format: [docs/guide/README.md](docs/guide/README.md)).

Documentation: [docs/guide/](docs/guide/index.md) (user guide, reference, developer guide).
Architecture overview: [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md). Roadmap:
[docs/ROADMAP.md](docs/ROADMAP.md).
