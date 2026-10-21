---
title: Versioning
description: Forge versions follow Pride Versioning (PROUD.DEFAULT.SHAME); bump them with pnpm version:bump and release by publishing a GitHub release.
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

## Where the version lives

The same version is written in three files:

- `Cargo.toml` (`[workspace.package] version`; every crate inherits it),
- `app/src-tauri/tauri.conf.json` (the bundles' version and what the updater compares),
- `app/ui/package.json`.

Never edit them by hand. From `app/ui`:

```sh
pnpm version:check                        # fails when the three files disagree
pnpm version:bump default                 # 0.1.0 → 0.2.0 in all three files + Cargo.lock
pnpm version:bump shame                   # 0.2.0 → 0.2.1
pnpm version:bump proud                   # 0.2.1 → 1.0.0
pnpm version:bump default --pre beta.1    # 1.0.0 → 1.1.0-beta.1
```

The script (`app/ui/scripts/version.mjs`, logic and tests in `versionBump.mjs`) updates
`Cargo.lock` for the workspace crates, prints the release tag and does nothing else: it never
commits, tags or publishes.

## Releasing a version

1. `pnpm version:bump <part>` and run the [quality gates](quality-gates.md).
2. Commit (`Release 0.2.0`) and merge to the release branch.
3. On GitHub, publish a release tagged `v0.2.0` (a pre-release for `-beta` versions).
4. The [release workflow](release-builds.md#official-releases) builds the bundles, signs them and
   uploads them with `latest.json`; installed copies offer the update on their next start.

> **Note:** Only the project owner bumps versions and publishes releases. Agents never bump a
> version unless they are asked to, and never publish releases.
