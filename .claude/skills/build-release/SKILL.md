---
name: build-release
description: Build a release binary or installer of Forge (AppImage, deb, rpm, dmg, msi) and smoke-test it. Use when asked for a runnable binary, an AppImage for Arch/EndeavourOS, or a release build.
---

# Build a release

## Build

From `app/ui`:

```sh
pnpm tauri build --bundles appimage   # Linux, runs on Arch/EndeavourOS/Ubuntu/Fedora
pnpm tauri build --bundles deb,rpm    # Linux packages
pnpm tauri build                      # all bundles for the current OS (dmg on macOS, msi/nsis on Windows)
```

`pnpm tauri` runs the Tauri CLI from `app/`; it builds the UI first (`pnpm build`).
Output: `target/release/bundle/<type>/` (e.g. `Forge_0.1.0_amd64.AppImage`) and the plain
binary `target/release/forge`. A release build takes about 6 minutes from scratch.

Cross-compiling is not supported; build Windows and macOS bundles on those systems (or with
the manual CI workflow, which the project owner starts).

## Remote development agents

Remote projects need the headless Forge agent for the host's architecture. Build the static
Linux agents before packaging (`rustup target add x86_64-unknown-linux-musl` first; aarch64
also needs a cross compiler, see the script):

```sh
pnpm --dir app/ui agents            # target/agents/forge-remote-agent-{x86_64,aarch64}-linux
```

Ship them in an `agents/` folder next to the `forge` binary (or set `FORGE_REMOTE_AGENT_DIR`);
Forge uploads the matching one to `~/.cache/forge-agent/<version>/` on the host.

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
