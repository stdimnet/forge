---
name: build-release
description: Build a release binary or installer of Forge (AppImage, deb, rpm, dmg, msi) and smoke-test it. Use when asked for a runnable binary, an AppImage for Arch/EndeavourOS, or a release build.
---

# Build a release

## Build

From `app/ui`:

```sh
pnpm release --bundles appimage      # Linux AppImage with the remote agents (see below)
pnpm tauri build --bundles appimage   # Linux AppImage without them
pnpm tauri build --bundles deb,rpm    # Linux packages
pnpm tauri build                      # all bundles for the current OS (dmg on macOS, msi/nsis on Windows)
```

`pnpm tauri` runs the Tauri CLI from `app/`; it builds the UI first (`pnpm build`).
Output: `target/release/bundle/<type>/` (e.g. `Forge_0.1.0_amd64.AppImage`) and the plain
binary `target/release/forge`. A release build takes about 6 minutes from scratch.

Cross-compiling is not supported; build Windows and macOS bundles on those systems. Official
builds come from `.github/workflows/release.yml`: it runs only when the project owner publishes a
GitHub release, builds Linux, Windows and macOS bundles with the remote agents, signs the
updater artifacts and uploads them together with `latest.json`, the feed Forge's updater reads.
Agents never publish releases or trigger workflows.

## Signed updates

`pnpm tauri` / `pnpm release` go through `app/ui/scripts/tauri.mjs`:

- `TAURI_SIGNING_PRIVATE_KEY` (+ `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`) set: updater artifacts
  (`.sig` next to each bundle) are built. Not set: they are turned off and the build still works.
- `FORGE_UPDATER_PUBKEY` set: the public key is built into the app. Not set: the build never
  checks for updates ("Updates are not configured in this build").

Create keys with `pnpm tauri signer generate -w ~/.tauri/forge.key`; never commit them.
Details: `docs/guide/developer/release-builds.md#updates-and-signing`.

## Remote development agents

Remote projects need the headless Forge agent for the host's architecture. Build the static
Linux agents before packaging (`rustup target add x86_64-unknown-linux-musl` first; aarch64
also needs a cross compiler, see the script):

```sh
pnpm --dir app/ui agents            # target/agents/forge-remote-agent-{x86_64,aarch64}-linux.gz
```

`pnpm release` builds the agents and then the bundles with `src-tauri/tauri.agents.conf.json`,
which ships `target/agents/` as the bundle's `agents/` resource folder (arguments go to
`tauri build`, e.g. `pnpm release --bundles appimage`). Architectures whose musl target is not
installed are skipped. Without bundled agents, put them in an `agents/` folder next to the
`forge` binary or set `FORGE_REMOTE_AGENT_DIR`. Forge uploads the matching one to
`~/.cache/forge-agent/<version>/` on the host.

## Smoke test on a headless Linux box

```sh
APPIMAGE_EXTRACT_AND_RUN=1 xvfb-run -a -s "-screen 0 1440x900x24" \
  sh -c './Forge_0.1.0_amd64.AppImage /path/to/project & sleep 15; import -window root shot.png'
```

Check the screenshot: the Welcome screen appears without a project argument; with a project
the workbench, file tree and git branch are shown. Make sure the run did not create files in the
project (`git status` stays clean).

## Running it (users)

```sh
chmod +x Forge_0.1.0_amd64.AppImage
./Forge_0.1.0_amd64.AppImage                 # needs fuse2 + webkit2gtk-4.1
APPIMAGE_EXTRACT_AND_RUN=1 ./Forge_0.1.0_amd64.AppImage   # without FUSE
```
