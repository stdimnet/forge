---
title: forge-update
description: "Forge update checks: when to check, which release is an update, the update feed, simulated updates and download speed."
order: 245
tags: [crate, rust, updates]
---

# forge-update

`crates/forge-update` — Forge update checks: when to check, which release is an update, the update feed, simulated updates and download speed.

## Purpose

The Tauri shell downloads, verifies (minisign signature) and installs updates with
`tauri-plugin-updater`. Everything that decides *whether* and *what* to offer lives here,
without a window or an app handle, so it is unit tested.

- `policy`: the update channel, the remembered state (last check, skipped and postponed
  versions), when a check is due (`plan`) and which release counts as an update (`evaluate`).
- `feed`: the `latest.json` feed that `pnpm tauri build` produces (`createUpdaterArtifacts`),
  fetching it, the download size and the `UpdateOffer` the splash screen shows.

## Channels

| Channel | Feeds read (`Channel::endpoints`) | Offered |
| --- | --- | --- |
| stable | `DEFAULT_ENDPOINT` | releases without a pre-release label |
| beta | `BETA_ENDPOINT`, then `DEFAULT_ENDPOINT` | the newest release of both feeds, pre-releases included |

`pick_feed` takes the newest release among the answers: a missing beta feed (404) is ignored as
long as the stable feed answers, and a feed marked `"channel": "beta"` never counts on the
stable channel. The offer remembers its feed (`UpdateOffer::feed`), so the download reads the
same one. An `updates.endpoint` override is the only feed read.
- `simulate`: a fake offer and download for `FORGE_SIMULATE_UPDATE` (desktop dev builds) and
  `?simulateUpdate` (browser mode). Nothing is installed.
- `meter`: the download speed shown while an update downloads.

## Modules

| Module | Summary |
| --- | --- |
| `policy.rs` | `Channel` (its feeds `endpoints`, `of_release` for tags), `UpdateState`, `plan` (once a day, dev builds, missing key, simulation) and `evaluate` (newer, channel, skipped, postponed). |
| `feed.rs` | `Feed` (`latest.json`, optional `channel` field), `pick_feed` (newest release among the channel's feeds), `check`, `fetch_feed`, `download_size`, `client`, `UpdateOffer` and its one-line summary. |
| `simulate.rs` | Simulated offer and chunk sizes of the fake download. |
| `meter.rs` | `SpeedMeter`: bytes per second over a two-second window. |

## Main public items

```rust
pub use feed::{check, client, fetch_feed, pick_feed, CheckOutcome, CheckRequest, Feed, FeedPlatform, UpdateOffer};
pub use meter::SpeedMeter;
pub use policy::{evaluate, plan, Channel, CheckPlan, PlanInput, Postponed, SkipReason, UpdateState, Verdict};
pub const DEFAULT_ENDPOINT: &str; // https://github.com/Reeperk/Forge/releases/latest/download/latest.json
pub const BETA_ENDPOINT: &str;    // https://github.com/Reeperk/Forge/releases/download/beta/latest-beta.json
pub const CHECK_INTERVAL_SECS: u64; // one day
pub fn parse_version(text: &str) -> Result<semver::Version> { … }
pub fn format_size(bytes: u64) -> String { … }
```

## Used by

- IPC: `updates_check`, `updates_respond`, `updates_status`, `updates_clear_skipped`, and the
  update step of the splash (`app/src-tauri/src/commands/updates.rs`, `app/src-tauri/src/startup.rs`)
- UI: `app/ui/src/features/updates`, `app/ui/src/splash`

## Dependencies

- Workspace crates: none
- External: `serde`, `serde_json`, `thiserror`, `semver`, `reqwest`

## Tests

5 source files; 26 unit tests. The feed tests run against a local HTTP server on `127.0.0.1`
(no network access).

```sh
cargo test -p forge-update
```
