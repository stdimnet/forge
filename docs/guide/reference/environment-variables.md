---
title: Environment variables
description: Environment variables Forge reads (FORGE_*, credentials fallbacks, Docker and Kubernetes, desktop detection) and the ones it sets for processes it starts.
order: 70
tags: [environment, variables, reference]
---

# Environment variables

## Read by Forge

### Forge's own

| Variable | Effect |
| --- | --- |
| `FORGE_NEW_WINDOW` | set by Forge when it starts another project window (Open Project › New Window): the instance skips the single-instance hand-over, the splash and the update check; it is removed from the environment at once, so terminals do not inherit it |
| `FORGE_GIT` | git executable to use instead of `git` on the `PATH` (e.g. a bundled git on Windows) |
| `FORGE_REMOTE_AGENT_DIR` | folder with the remote agent binaries (`forge-remote-agent-<arch>-linux[.gz]`), searched before the bundle's `agents/` folder |
| `FORGE_AGENT_LOG` | log filter of the remote agent on the host (e.g. `debug`); the agent logs to stderr |
| `RUST_LOG` | log filter of the desktop app (e.g. `RUST_LOG=forge_lsp=debug`; `forge_app_lib::startup=debug` logs the splash steps) |
| `FORGE_SIMULATE_UPDATE` | e.g. `0.2.0`: the splash and Help \| Check for Updates… offer this version as a simulated update with a fake download; nothing is installed (for development, works in any build) |
| `APPIMAGE` | set by the AppImage runtime; Forge then registers the `forge://` scheme at startup |
| `APPIMAGE_EXTRACT_AND_RUN` | read by the AppImage runtime: `1` runs without FUSE (see [Installation](../getting-started/installation.md#running-the-appimage-without-fuse)) |

### Credentials (fallbacks when nothing is stored in the keychain)

| Variable | Used for |
| --- | --- |
| `ANTHROPIC_API_KEY` | Anthropic provider |
| `OPENAI_API_KEY` | OpenAI provider |
| `OPENROUTER_API_KEY` | OpenRouter provider |
| `GITLAB_TOKEN` | gitlab.com |
| `FORGEJO_TOKEN`, `GITEA_TOKEN` | Forgejo / Gitea (public servers) |
| `GITHUB_TOKEN`, `GH_TOKEN` | github.com |

### Tools and environment

| Variable | Used for |
| --- | --- |
| `PATH`, `PATHEXT` | finding git, php, composer, node, language servers, agent CLIs |
| `SHELL` | the terminal's default shell (Linux/macOS) |
| `ComSpec` | the last-resort terminal shell on Windows |
| `HOME`, `USERPROFILE`, `APPDATA` | home and config folders |
| `NPM_CONFIG_PREFIX` | npm's global folder when looking for language servers |
| `VIMRUNTIME` | Neovim runtime for lua-language-server's Neovim environment |
| `DOCKER_HOST`, `DOCKER_CONTEXT` | the Docker daemon (also `ssh://` hosts) or Docker context when `docker.connection` is `auto` |
| `XDG_RUNTIME_DIR` | rootless Docker socket |
| `KUBECONFIG` | kubeconfig when `kubernetes.kubeconfig` is empty |
| `GIT_SSH_COMMAND` | kept if set; otherwise Forge runs git with `ssh -o BatchMode=yes` |
| `XDG_CURRENT_DESKTOP`, `XDG_SESSION_DESKTOP`, `DESKTOP_SESSION`, `KDE_FULL_SESSION`, `GNOME_DESKTOP_SESSION_ID` | detecting the desktop for "Window controls: Match the system" |
| `USER`, `USERNAME`, `HOSTNAME` | default values (SSH user, display names) |

## Set by Forge for processes it starts

| Variable | When |
| --- | --- |
| `XDEBUG_MODE=debug`, `XDEBUG_SESSION=FORGE`, `XDEBUG_CONFIG=client_host=127.0.0.1 client_port=<port> idekey=FORGE`, `PHP_IDE_CONFIG=serverName=<server>` | Debug of a PHP run configuration |
| `NODE_OPTIONS=<web.npm.debugArguments>` | Debug of an npm script |
| the entries of `terminal.environment` | new terminal tabs |
| the configuration's **Environment variables** | run configurations |
| `GIT_SSH_COMMAND=ssh -o BatchMode=yes` | git commands, unless already set |

## Build and development

| Variable | Effect |
| --- | --- |
| `FORGE_UPDATE_SNAPSHOTS=1` | `cargo test -p forge-app` rewrites the browser-mode snapshots of the plugin manifests and project generators |
| `FORGE_URL` | base URL of the dev server for `scripts/screenshots.mjs` (default `http://localhost:1420`) |
| `CHROMIUM_PATH` | Chromium for the screenshot script |
| `CARGO_TARGET_DIR` | separate target folder per git worktree |
| `CC_<triple>`, `CFLAGS_<triple>`, `CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER` | cross-compiling the remote agents (`pnpm agents`) |
| `FORGE_UPDATER_PUBKEY` | at build time: the updater public key compiled into the app; without it the build never checks for updates (see [Release builds](../developer/release-builds.md#updates-and-signing)) |
| `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | at build time: sign the updater artifacts; `pnpm tauri build` / `pnpm release` turn on `createUpdaterArtifacts` only when the key is set |
