# Forge

A fast, memory-safe IDE in the spirit of PHPStorm — built with **Rust + Tauri 2** and a
**SolidJS** UI. Runs on Windows, Linux and macOS.

Forge is the core. Languages and big feature sets ship as **packs** of plugins:

| Pack | Focus |
| --- | --- |
| **PHPForge** | PHP, Laravel, Symfony, Doctrine (DQL), Composer, PHPUnit, Xdebug |
| **WebForge** | HTML, CSS, JavaScript/TypeScript, Vue/React, Node |
| **LuaForge** | Lua |
| **AIForge** | AI agents: Claude Code, Gemini CLI, any OpenAI-compatible model; orchestrate several agents in parallel |

The layout and keymap follow PHPStorm 1:1 so you feel at home immediately; the look is Forge's own.

![Forge workbench](docs/screenshots/workbench-dark.png)

## Development

Requirements: Rust (stable), Node 22 + pnpm, and on Linux the
[Tauri system dependencies](https://tauri.app/start/prerequisites/).

```sh
cd app/ui && pnpm install
pnpm tauri dev            # desktop app
pnpm dev                  # UI only in the browser, backed by a mocked demo project
pnpm screenshots          # capture docs/screenshots (needs `pnpm dev` running)
cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for step-by-step guides (also usable as coding-agent
skills in `.claude/skills/`), [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) and
[docs/ROADMAP.md](docs/ROADMAP.md).

## License

MIT. Third-party language servers (e.g. Intelephense premium) and AI providers require
your own license or API key.
