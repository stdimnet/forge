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

Check the screenshot: without a project argument the Welcome screen appears; with one, the
workbench, file tree and Git branch. Make sure the run created no files in the project
(`git status` stays clean).

## Versioning

The version (0.1.0) is set in `[workspace.package]` of the root `Cargo.toml`,
`app/src-tauri/tauri.conf.json` and `app/ui/package.json`. The bundle identifier is
`dev.forge.ide`; the `forge://` scheme is declared under `plugins.deep-link` in
`tauri.conf.json`.
