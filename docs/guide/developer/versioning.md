---
title: Versioning
description: Forge versions follow Pride Versioning (PROUD.DEFAULT.SHAME); bump them with pnpm version:bump and release by pushing a version tag.
order: 145
tags: [release, version, pridever, updates]
---

# Versioning

Forge versions follow [Pride Versioning](https://pridever.org) 0.3.0: **PROUD.DEFAULT.SHAME**.

| Part | Bump it for | Example |
| --- | --- | --- |
| PROUD | a release we are really proud of; DEFAULT and SHAME go back to 0 | `1.2.3` → `2.0.0` |
| DEFAULT | a normal release; SHAME goes back to 0 | `1.2.3` → `1.3.0` |
| SHAME | a fix too embarrassing to admit | `1.2.3` → `1.2.4` |

The written form stays `X.Y.Z`, so Cargo, npm and Tauri's updater parse and order versions like
SemVer. Pre-releases use a SemVer label (`0.2.0-beta.1`); they go to the beta update channel
(`updates.channel`).

### MSI version

The Windows MSI installer (WiX) accepts only numeric versions `major.minor.build[.revision]`
(at most 255.255.65535.65535), not `0.2.0-beta.1`. `pnpm tauri build` therefore passes a
derived `bundle.windows.wix.version` on Windows (`app/ui/scripts/tauriArgs.mjs`); the app, the
NSIS installer and the updater keep the SemVer version.

| App version | MSI version |
| --- | --- |
| `0.1.1` | `0.1.1` |
| `0.1.1-beta.1`, `0.1.1-alpha.1`, `0.1.1-rc.1` | `0.1.1.1` |
| `0.1.1-rc.2` | `0.1.1.2` |
| `0.1.1-7` | `0.1.1.7` |
| `0.1.1-beta` | `0.1.1.0` |

The label is dropped and its number becomes the fourth field. Windows Installer compares only
the first three fields for an upgrade, so every pre-release of `0.1.1` and the final `0.1.1`
count as the same version. They still install over each other because the MSI allows
downgrades (`bundle.windows.allowDowngrades`, Tauri's default, which makes WiX's `MajorUpgrade`
remove any installed Forge, the same version included): `beta.1` → `beta.2` → `rc.1` → `0.1.1`
each replace the previous install. A later version (`0.1.2-beta.1` → `0.1.2.1`) is an ordinary
upgrade.

So a version must stay MSI-compatible: PROUD and DEFAULT at most 255, SHAME at most 65535, and
a pre-release label of the form `<label>.<N>` (`beta.1`, `rc.2`), a bare number or a bare label,
with `N` at most 65535. `pnpm version:bump` refuses anything else (`beta.1.2`, `beta.70000`), and
`pnpm version:check` fails on it and when `tauri.conf.json` pins `bundle.windows.wix.version` or
turns `allowDowngrades` off. `pnpm workflows:check` makes sure the workflows build through
`pnpm tauri` (tauri-action's `tauriScript: pnpm --dir ui tauri`), which adds the MSI version.

## Where the version lives

The same version is written in three files:

- `Cargo.toml` (`[workspace.package] version`; every crate inherits it),
- `app/src-tauri/tauri.conf.json` (the bundles' version and what the updater compares),
- `app/ui/package.json`.

Never edit them by hand. From `app/ui`:

```sh
pnpm version:check                        # fails when the files disagree or there is no MSI version
pnpm version:bump default                 # 0.1.0 → 0.2.0 in all three files + Cargo.lock
pnpm version:bump shame                   # 0.2.0 → 0.2.1
pnpm version:bump proud                   # 0.2.1 → 1.0.0
pnpm version:bump default --pre beta.1    # 1.0.0 → 1.1.0-beta.1
pnpm version:bump pre                     # 1.1.0-beta.1 → 1.1.0-beta.2
pnpm version:bump pre --pre rc.1          # 1.1.0-beta.2 → 1.1.0-rc.1
pnpm version:bump release                 # 1.1.0-rc.1 → 1.1.0 (the final release)
```

`proud`, `default` and `shame` always raise a number, also on a pre-release
(`1.1.0-rc.1` → `shame` → `1.1.1`). `pre` keeps PROUD.DEFAULT.SHAME and only moves the
pre-release label on: without `--pre` it adds one to the label's last number (`beta` becomes
`beta.1`); with `--pre` it takes the new label, which must come after the current one in SemVer
order (`beta.2` → `rc.1`, not back to `beta.1`). `pre` and `release` work only on a pre-release
version.

The script (`app/ui/scripts/version.mjs`, logic and tests in `versionBump.mjs`) updates
`Cargo.lock` for the workspace crates, prints the release tag and does nothing else: it never
commits, tags or publishes.

## Releasing a version

1. `pnpm version:bump <part>` and run the [quality gates](quality-gates.md).
2. Commit (`Release 0.2.0`) and merge to the release branch.
3. Tag that commit and push the tag: `git tag v0.2.0 && git push origin v0.2.0`.
4. The [release workflow](release-builds.md#official-releases) creates the release (a
   pre-release for `-beta` versions), builds the bundles, signs them, uploads them with
   `latest.json` and `SHA256SUMS` and publishes it; a pre-release also updates the beta feed
   ([Channels](release-builds.md#channels)). Installed copies offer the update on their next
   start.
5. For a stable release, run `pnpm packaging:update v0.2.0` and commit the AUR and Flatpak
   changes ([Linux packages](release-builds.md#linux-packages)).

> **Note:** Only the project owner bumps versions and pushes version tags. Agents never bump a
> version unless they are asked to, and never push version tags or publish releases.
