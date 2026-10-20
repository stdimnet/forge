---
title: Release builds
description: Build Forge bundles (AppImage, deb, rpm, dmg, msi/nsis), bundle the remote agents with pnpm release, and smoke-test a build.
order: 140
tags: [release, build, appimage, bundles, remote agent]
---

# Release builds

Recipe: [`build-release`](../../../.claude/skills/build-release/SKILL.md).

## Bundles

From `app/ui`:

```sh
pnpm release --bundles appimage       # Linux AppImage with the remote agents
pnpm tauri build --bundles appimage   # Linux AppImage without them
pnpm tauri build --bundles deb,rpm    # Linux packages
pnpm tauri build                      # all bundles for this OS (dmg on macOS, msi/nsis on Windows)
```

`pnpm tauri` runs the Tauri CLI from `app/`; it builds the UI first (`pnpm build`). Output:
`target/release/bundle/<type>/` (e.g. `Forge_0.1.0_amd64.AppImage`) and the plain binary
`target/release/forge`. A build from scratch takes about six minutes.

Cross-compiling is not supported: build Windows and macOS bundles on those systems.

## Official releases

Publishing a GitHub release runs `.github/workflows/release.yml`, and nothing else does. It:

1. builds the static remote agents on Linux,
2. builds the bundles on Ubuntu (AppImage, deb, rpm), Windows (NSIS, MSI) and macOS (Apple
   Silicon and Intel) with `pnpm tauri build --config src-tauri/tauri.agents.conf.json`,
3. signs the updater artifacts and uploads everything plus `latest.json` to the release.

Forge's updater reads `https://github.com/Reeperk/Forge/releases/latest/download/latest.json`,
so a published (not draft, not pre-release) release is what users get offered.

The workflow needs, once, in the repository settings:

| Name | Kind | Value |
| --- | --- | --- |
| `TAURI_SIGNING_PRIVATE_KEY` | secret | private key from `pnpm tauri signer generate` |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | secret | its password |
| `FORGE_UPDATER_PUBKEY` | variable | the matching public key, built into the app |

Keep the private key offline as well: losing it means installed copies can no longer verify
updates.

## Updates and signing

Forge updates itself with `tauri-plugin-updater`: it reads `latest.json`, downloads the bundle
for its platform, verifies the bundle's minisign signature against the public key built into
the app, installs it and restarts. When and what to offer (once a day, skipped versions,
channel, version compare) is decided in [`forge-update`](crates/forge-update.md); the shell side
is `app/src-tauri/src/commands/updates.rs` and the splash steps are in
`app/src-tauri/src/startup.rs`.

### Create the key pair (once)

```sh
cd app/ui
pnpm tauri signer generate -w ~/.tauri/forge.key   # asks for a password
```

This writes the private key `~/.tauri/forge.key` and the public key `~/.tauri/forge.key.pub`.

- Store the private key and its password as the secrets `TAURI_SIGNING_PRIVATE_KEY` and
  `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` (repository settings), and in a password manager.
- Store the public key (the contents of `forge.key.pub`) as the variable `FORGE_UPDATER_PUBKEY`.
- Never commit either file. The public key is not secret, but it lives in the build
  environment, not in the repository, so forks and local builds do not pretend to be official.

### How the keys reach a build

`pnpm tauri` runs `app/ui/scripts/tauri.mjs`, which passes everything to the Tauri CLI and adds
one `--config` to `tauri build` (applied last):

| Build environment | Effect |
| --- | --- |
| `TAURI_SIGNING_PRIVATE_KEY` set | `bundle.createUpdaterArtifacts` on: every bundle gets a signed `.sig` next to it |
| `TAURI_SIGNING_PRIVATE_KEY` not set | `createUpdaterArtifacts` off, so the build succeeds without the key (even though `tauri.agents.conf.json` turns it on for releases) |
| `FORGE_UPDATER_PUBKEY` set | written to `plugins.updater.pubkey`; the Rust code also compiles it in (`option_env!`) |
| `FORGE_UPDATER_PUBKEY` not set | the app never checks for updates: it logs "update checks are disabled: this build has no updater public key" and the Updates settings page says "Updates are not configured in this build" |

The release workflow's call `pnpm tauri build --config src-tauri/tauri.agents.conf.json` goes
through the same script, as does `pnpm release`. A local signed build:

```sh
export TAURI_SIGNING_PRIVATE_KEY="$(cat ~/.tauri/forge.key)"
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD='…'
export FORGE_UPDATER_PUBKEY="$(cat ~/.tauri/forge.key.pub)"
pnpm release --bundles appimage   # Forge_0.1.0_amd64.AppImage + Forge_0.1.0_amd64.AppImage.sig
```

### latest.json

The workflow's `tauri-action` (`includeUpdaterJson: true`) writes `latest.json` from the `.sig`
files and attaches it to the release. Its format, for a feed you publish yourself:

```json
{
  "version": "0.2.0",
  "notes": "What changed.",
  "pub_date": "2026-11-02T10:00:00Z",
  "platforms": {
    "linux-x86_64": { "signature": "<contents of the .AppImage.sig>", "url": "https://…/Forge_0.2.0_amd64.AppImage" },
    "windows-x86_64": { "signature": "…", "url": "https://…/Forge_0.2.0_x64-setup.exe" },
    "darwin-aarch64": { "signature": "…", "url": "https://…/Forge.app.tar.gz" }
  }
}
```

Keys are `{os}-{arch}` or `{os}-{arch}-{installer}` (`linux-x86_64-deb`); the more specific one
wins. Forge also sends a `HEAD` request to the download to show its size.

| Setting | Feed |
| --- | --- |
| `updates.channel = stable` (default) | `https://github.com/Reeperk/Forge/releases/latest/download/latest.json` |
| `updates.channel = beta` | `https://github.com/Reeperk/Forge/releases/download/beta/latest.json` (a release tagged `beta` whose `latest.json` is replaced for each pre-release) |
| `updates.endpoint` | any other URL; release builds require `https` |

On the stable channel versions with a pre-release part (`0.3.0-beta.1`) are never offered.

> **Note:** the release workflow publishes the stable feed only; nothing maintains the `beta`
> release yet, so the beta channel finds no feed (the check reports an error) until one exists.

### Trying the update flow without a release

Development builds never check for real updates. Simulate one instead; it shows the update
panel with a fake 94 MB download and installs nothing:

```sh
FORGE_SIMULATE_UPDATE=0.2.0 pnpm tauri dev
```

In browser mode: `http://localhost:1420/?splash&simulateUpdate=0.2.0` (the splash, then the
workbench) or `?simulateUpdate=0.2.0` with Help | Check for Updates…. To test a real update,
build a signed release of the old version, publish a feed with a newer signed build on a local
`https` server and point `updates.endpoint` at it.

## Remote agents

Remote projects need the headless agent (`forge-remote-agent`) for the host's architecture. Build
the static Linux agents first:

```sh
rustup target add x86_64-unknown-linux-musl          # aarch64-unknown-linux-musl as well
pnpm --dir app/ui agents                              # or: pnpm agents x86_64
```

This writes `target/agents/forge-remote-agent-{x86_64,aarch64}-linux.gz` (gzip-compressed,
because AppImage tools would patch an uncompressed ELF file in the bundle). Architectures whose
musl target is not installed are skipped. aarch64 needs a cross compiler and linker
(`CC_aarch64_unknown_linux_musl`, `CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER`); the
tree-sitter C code of the native architecture is built with the host's `gcc` (fortify off).

`pnpm release` builds the agents and then the bundles with
`src-tauri/tauri.agents.conf.json`, which ships `target/agents/` as the bundle's `agents/`
resource folder. Arguments go to `tauri build`.

At runtime Forge looks for agents in `FORGE_REMOTE_AGENT_DIR`, the bundle's `agents/`
resources, `agents/` next to its executable and `target/agents`; a development build of
`forge-remote-agent` next to the executable serves hosts of the same architecture. Plain or
`.gz` binaries are accepted. The agent is uploaded to `~/.cache/forge-agent/<version>/` on the
host.

## Smoke test on a headless Linux box

```sh
APPIMAGE_EXTRACT_AND_RUN=1 xvfb-run -a -s "-screen 0 1440x900x24" \
  sh -c './Forge_0.1.0_amd64.AppImage /path/to/project & sleep 15; import -window root shot.png'
```

Check the screenshot: the splash appears first; then, without a project argument, the last
project or the Welcome screen; with one, the workbench, file tree and Git branch. Use a throwaway
`HOME` (or `XDG_CONFIG_HOME`) so the run does not reopen your own last project. Make sure the run created no files in the project
(`git status` stays clean).

## Versioning

The version (0.1.0) is set in `[workspace.package]` of the root `Cargo.toml`,
`app/src-tauri/tauri.conf.json` and `app/ui/package.json`. The bundle identifier is
`dev.forge.ide`; the `forge://` scheme is declared under `plugins.deep-link` in
`tauri.conf.json`.
