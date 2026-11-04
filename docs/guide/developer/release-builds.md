---
title: Release builds
description: Build Forge bundles (AppImage, deb, rpm, dmg, msi/nsis), sign them (Authenticode, Apple notarisation), publish stable and beta update feeds, package Flatpak and AUR, and smoke-test a build.
order: 140
tags: [release, build, appimage, bundles, remote agent, signing, notarisation, flatpak, aur, beta]
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

Versions follow Pride Versioning and are bumped with `pnpm version:bump`; see
[Versioning](versioning.md).

Pushing a version tag (`git tag v0.2.0 && git push origin v0.2.0`) runs
`.github/workflows/release.yml`, and nothing else does (`pnpm workflows:check` in the
[quality gates](quality-gates.md) fails if a workflow gets another trigger). It:

1. `meta`: decides the channel from the tag (`vX.Y.Z` stable, `vX.Y.Z-beta.N` beta), stops
   when the tag does not match the app version in `tauri.conf.json`, and creates a draft
   release `Forge <version>` with generated notes (a pre-release for the beta channel); a
   re-run of the same tag reuses that draft,
2. builds the static remote agents on Linux,
3. builds the bundles on Ubuntu (AppImage, deb, rpm), Windows (NSIS, MSI) and macOS (Apple
   Silicon and Intel) with `pnpm tauri build --config src-tauri/tauri.agents.conf.json`,
   [signing](#code-signing) them when the certificates are configured,
4. signs the updater artifacts and uploads everything plus `latest.json` to the draft,
5. `checksums`: uploads `SHA256SUMS` (and `SHA256SUMS.asc` with a GPG key) and, for a
   pre-release, the beta feed `latest-beta.json` (see [Channels](#channels)),
6. `publish`: publishes the release once every asset is uploaded. If a job fails, the release
   stays a draft; fix the cause and re-run the workflow.

Forge's updater reads `https://github.com/Reeperk/Forge/releases/latest/download/latest.json`,
so a published (not draft, not pre-release) release is what stable users get offered.

Linux build jobs (remote agents, Linux bundles) run on the project's self-hosted runner
(labels `self-hosted`, `Linux`, `X64`); Windows and macOS bundles and the small `meta`,
`checksums` and `publish` jobs run on GitHub's runners (`checksums` needs `gh` and `gpg`). The self-hosted
runner needs the Debian packages listed in `.github/scripts/linux-deps.sh`. The script installs
missing ones when the runner is root or has passwordless sudo; otherwise the job stops and
prints the `apt-get install` command to run once as root on the runner.

### Secrets and variables

Set them once in the repository settings (Settings | Secrets and variables | Actions). Only the
updater key is needed for a working release; every other row is optional, and a missing one
skips that step with a notice in the log. Never commit certificates or keys.

| Name | Kind | Value |
| --- | --- | --- |
| `TAURI_SIGNING_PRIVATE_KEY` | secret | updater private key from `pnpm tauri signer generate` |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | secret | its password |
| `FORGE_UPDATER_PUBKEY` | secret or variable | the matching public key, built into the app |
| `WINDOWS_CERTIFICATE` | secret | Authenticode certificate: the `.pfx` file, base64-encoded |
| `WINDOWS_CERTIFICATE_PASSWORD` | secret | the `.pfx` password |
| `AZURE_CLIENT_ID`, `AZURE_CLIENT_SECRET`, `AZURE_TENANT_ID` | secrets | Azure Trusted Signing: the app registration allowed to sign |
| `AZURE_SIGNING_ENDPOINT`, `AZURE_SIGNING_ACCOUNT`, `AZURE_CERTIFICATE_PROFILE` | variables | Azure Trusted Signing: endpoint (e.g. `https://weu.codesigning.azure.net`), account and certificate profile |
| `APPLE_CERTIFICATE` | secret | "Developer ID Application" certificate: the `.p12` file, base64-encoded |
| `APPLE_CERTIFICATE_PASSWORD` | secret | the `.p12` password |
| `APPLE_SIGNING_IDENTITY` | secret | e.g. `Developer ID Application: Jane Doe (TEAMID1234)` |
| `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID` | secrets | notarisation with an Apple ID and an app-specific password |
| `APPLE_API_KEY`, `APPLE_API_ISSUER`, `APPLE_API_KEY_P8` | secrets | notarisation with an App Store Connect API key instead: key ID, issuer ID and the contents of `AuthKey_<id>.p8` |
| `RELEASE_GPG_PRIVATE_KEY`, `RELEASE_GPG_PASSPHRASE` | secrets | armored private key and passphrase for `SHA256SUMS.asc` |

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
- Store the public key (the contents of `forge.key.pub`) as the secret (or variable) `FORGE_UPDATER_PUBKEY`.
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
wins. Forge also sends a `HEAD` request to the download to show its size. An optional
`"channel": "beta"` marks a beta feed, which installations on the stable channel ignore.

The channels and their feeds are described in [Channels](#channels).

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

## Code signing

Signing happens in the `bundles` job, before `tauri-action` builds; the Tauri bundler then
signs the binaries and installers itself. Nothing is signed in local builds unless you set the
same environment variables.

### Windows (Authenticode)

Two ways; the workflow uses the first one that is configured.

**A `.pfx` certificate (OV or EV exported with its key)** is the default. Encode it and store
it with its password:

```sh
base64 -w0 forge-codesign.pfx > forge-codesign.pfx.b64   # paste into WINDOWS_CERTIFICATE
```

The workflow imports it into the runner's `CurrentUser\My` store and sets
`FORGE_WINDOWS_CERT_THUMBPRINT`. `app/ui/scripts/tauriArgs.mjs` turns that into
`bundle.windows.certificateThumbprint` (with `digestAlgorithm: sha256` and the timestamp server
`http://timestamp.digicert.com`, or `FORGE_WINDOWS_TIMESTAMP_URL`), and Tauri runs `signtool`
on `forge.exe`, the NSIS installer and the MSI. Locally on Windows, import the certificate and
set `FORGE_WINDOWS_CERT_THUMBPRINT` before `pnpm tauri build`.

**Azure Trusted Signing** suits certificates that cannot be exported (most EV certificates
since 2023). Set the three `AZURE_*` secrets and three variables above. The workflow installs
`trusted-signing-cli` (`cargo install`) and sets `FORGE_AZURE_SIGNING_ENDPOINT`, `_ACCOUNT` and
`_PROFILE`; `tauriArgs.mjs` sets `bundle.windows.signCommand` to
`trusted-signing-cli -e <endpoint> -a <account> -c <profile> -d Forge %1`, which Tauri calls
for every file it signs.

Check a signed build with `Get-AuthenticodeSignature .\Forge_0.2.0_x64-setup.exe`.

### macOS (Developer ID and notarisation)

The app is signed with the hardened runtime and `app/src-tauri/Entitlements.plist`
(`bundle.macOS.entitlements`), which allows JIT and unsigned executable memory for the webview's
JavaScript engine. Forge is not sandboxed. Tauri reads the `APPLE_*` variables: it imports
`APPLE_CERTIFICATE` into a temporary keychain, signs with `APPLE_SIGNING_IDENTITY`, submits the
app with `notarytool`, waits for the result and staples the ticket. The workflow exports only
the secrets that exist:

| Configured | Result |
| --- | --- |
| no certificate or identity | unsigned (Gatekeeper asks users to allow the app in System Settings) |
| certificate + identity | signed, not notarised (a warning in the log) |
| + `APPLE_API_KEY`, `APPLE_API_ISSUER`, `APPLE_API_KEY_P8` | signed and notarised with an API key (preferred) |
| + `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID` | signed and notarised with an Apple ID |

Export the certificate from Keychain Access as `.p12` and encode it with
`base64 -i cert.p12 | pbcopy`. Check a build with `codesign -dv --verbose=4 Forge.app`,
`spctl -a -vv Forge.app` and `xcrun stapler validate Forge.app`.

## Checksums

The `checksums` job downloads every asset of the release, writes `SHA256SUMS` (the output of
`sha256sum`) and uploads it. With `RELEASE_GPG_PRIVATE_KEY` it also uploads the detached
signature `SHA256SUMS.asc`. Users verify a download with:

```sh
sha256sum --ignore-missing -c SHA256SUMS
gpg --verify SHA256SUMS.asc SHA256SUMS   # with the project's public key
```

The AUR and Flatpak packages take their checksums from this file.

## Channels

Forge has two update channels, chosen with "Update channel" (`updates.channel`) under
Settings | Appearance & Behavior | System Settings | Updates. The logic is in
[`forge-update`](crates/forge-update.md) (`Channel::endpoints`, `pick_feed`).

| Release | Tag | Published as | Feed |
| --- | --- | --- | --- |
| stable | `v0.2.0` | release | `latest.json` of the release; GitHub's `releases/latest` points at it |
| beta | `v0.3.0-beta.1` | pre-release | `latest.json` and `latest-beta.json` (`"channel": "beta"`) of the pre-release, and `latest-beta.json` of the release tagged `beta` |

| Setting | Feeds read | Offered |
| --- | --- | --- |
| `updates.channel = stable` (default) | `https://github.com/Reeperk/Forge/releases/latest/download/latest.json` | stable releases only |
| `updates.channel = beta` | `https://github.com/Reeperk/Forge/releases/download/beta/latest-beta.json`, then the stable feed | the newer of both, so a stable `0.3.0` replaces `0.3.0-beta.2` |
| `updates.endpoint` set | that URL only (release builds require `https`) | as the channel allows |

The beta feed needs a fixed URL, so it lives on a long-lived release tagged `beta`. Create it
once on GitHub: tag `beta`, title "Beta update feed", marked as **pre-release**, no assets.
The tag does not match `v*`, so it starts no workflow. From then on every pre-release replaces
its `latest-beta.json`. Until it exists, beta
installations simply get stable releases (the workflow prints a warning).

Pre-release versions come from `pnpm version:bump default --pre beta.1` and the next ones from
`pnpm version:bump pre` (see [Versioning](versioning.md)). The `meta` job creates a tag with a
`-label` as a pre-release, so a beta never becomes GitHub's "latest".

## Linux packages

### Flatpak

`packaging/flatpak/` holds the manifest `dev.forge.ide.yml`, the desktop file and the AppStream
metainfo. The manifest repackages the release's `.deb` (the same binary as the AppImage) on the
GNOME runtime, which ships webkit2gtk-4.1; it needs no Rust or Node toolchain and no generated
cargo/pnpm source lists, so a release only changes one URL and checksum. A source build with
`flatpak-cargo-generator` and `flatpak-node-generator` is what Flathub would prefer; it is
possible later, but means regenerating thousands of source entries for every release.

Permissions (`finish-args`) and why Forge needs them:

| Permission | Why |
| --- | --- |
| `--socket=wayland`, `--socket=fallback-x11`, `--share=ipc`, `--device=dri` | the window and GPU-accelerated webview |
| `--share=network` | AI providers, Git remotes, Composer/npm, update checks, remote development |
| `--filesystem=home`, `/media`, `/run/media`, `/mnt` | projects can live anywhere in the home directory or on other drives |
| `--socket=ssh-auth` | Git over SSH and SSH remote development use the user's agent |
| `--talk-name=org.freedesktop.secrets` | AI keys and passwords are stored in the Secret Service (keyring) |
| `--talk-name=org.freedesktop.Notifications` | notifications |
| `--talk-name=org.freedesktop.Flatpak` | `flatpak-spawn --host` to run PHP, Composer, Node, language servers, Docker and the terminal shell, which are installed on the host |

> **Note:** Forge does not yet route its tool runs through `flatpak-spawn --host`; inside the
> Flatpak it finds only the tools of the GNOME runtime. Until it does, the Flatpak suits
> editing and Git; prefer the AppImage, the `.deb` or the AUR package for full tool support.
> Updates of a Flatpak come from Flatpak itself, not from Forge's updater.

Build it locally (needs `flatpak-builder` and `flatpak install --user flathub
org.gnome.Platform//48 org.gnome.Sdk//48`; without `flatpak-builder` the script says so and
exits):

```sh
pnpm --dir app/ui flatpak                                   # the release .deb from the manifest
pnpm --dir app/ui flatpak target/release/bundle/deb/Forge_0.1.0_amd64.deb   # a local build
flatpak install --user target/flatpak/Forge.flatpak
appstreamcli validate packaging/flatpak/dev.forge.ide.metainfo.xml
desktop-file-validate packaging/flatpak/dev.forge.ide.desktop
```

The manifest carries `x-checker-data`, so on Flathub the external data checker proposes the
update for each stable release.

### AUR

`packaging/aur/forge-bin/PKGBUILD` repackages the release's `.deb`; `packaging/aur/forge-git`
builds the `main` branch from source (`pnpm tauri build --no-bundle`). Both follow stable
releases only; the AUR has no beta package.

After a stable release, point the packages at it:

```sh
pnpm --dir app/ui packaging:update v0.2.0            # fetches the release's SHA256SUMS
pnpm --dir app/ui packaging:update v0.2.0 --sums ~/Downloads/SHA256SUMS   # offline
```

This sets `pkgver` (and `pkgrel=1`) and the `.deb` checksum in `forge-bin/PKGBUILD`, points the
Flatpak manifest at the same `.deb`, adds the release to the metainfo and regenerates both
`.SRCINFO` files (`pnpm --dir app/ui packaging:srcinfo` does only the last step). Commit the
changes.

Publishing to the AUR is manual and never done by agents or workflows. With an AUR account and
SSH key (on an Arch system, so `makepkg` can check the result):

```sh
git clone ssh://aur@aur.archlinux.org/forge-bin.git aur-forge-bin
cp packaging/aur/forge-bin/{PKGBUILD,.SRCINFO} aur-forge-bin/
cd aur-forge-bin
makepkg -si                                   # builds and installs it as a test
makepkg --printsrcinfo | diff - .SRCINFO      # must be empty
git commit -am "Update to 0.2.0" && git push
```

The first upload creates the package; check beforehand that the name `forge-bin` is still free
on the AUR. `forge-git` is published the same way; its `pkgver` is computed by `makepkg`, so
regenerate its `.SRCINFO` there.

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
