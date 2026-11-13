---
title: javaforge-jdk
description: "JavaForge: JDK detection (JAVA_HOME, PATH, system folders, Windows registry and Program Files, SDKMAN, asdf, jabba, Gradle toolchains, IntelliJ's ~/.jdks) from each JDK's release file, and JDK downloads from Eclipse Adoptium (Temurin) with SHA-256 verification."
order: 272
tags: [crate, rust, pack]
---

# javaforge-jdk

`packs/javaforge/jdk` — JavaForge: JDK detection and Eclipse Temurin downloads.

## Purpose

JavaForge's JDKs, like IntelliJ IDEA's SDK table.

- `detect` finds the JDKs on the machine: `JAVA_HOME`, `java` on `PATH` (symlinks resolved),
  `/usr/lib/jvm`, `/usr/java`, `/opt/java` (Linux), `/Library/Java/JavaVirtualMachines` and
  `~/Library/Java/JavaVirtualMachines` plus Homebrew (macOS), `Program Files` vendor folders and
  the registry (`reg query`, Windows), SDKMAN!, asdf, jabba, Gradle toolchains (`~/.gradle/jdks`),
  IntelliJ's `~/.jdks` and Forge's downloads (`<data dir>/forge/jdks`). macOS bundles
  (`Contents/Home`) and folders with one JDK inside are followed; a JRE (no `javac`) is skipped;
  the same JDK reached twice is listed once. `pick` chooses the JDK of a minimum release
  (JDT LS) or a preferred one.
- `release` reads a JDK's `release` file (`JAVA_VERSION`, `IMPLEMENTOR`, …): the feature
  release (`1.8.0_412` → 8) and the vendor (`Eclipse Temurin`, `Azul Zulu`, …).
- `adoptium` downloads Eclipse Temurin: the Adoptium API names the latest build of a feature
  release for this platform with its SHA-256; the archive is verified, unpacked into a staging
  folder and renamed into `<data dir>/forge/jdks/<release>`.

## Modules

| Module | Summary |
| --- | --- |
| `adoptium.rs` | `AvailableReleases`, `Platform`, `assets_url`, `parse_assets`, `available_releases`, `download`. |
| `detect.rs` | `Jdk`, `DetectEnv` (`system()`), `detect`, `inspect`, `pick`, `parse_registry`, `system_roots`. |
| `release.rs` | `ReleaseInfo`, `parse_release`, `major_of`, `vendor_name`. |

## Main public items

```rust
pub use detect::{detect, inspect, pick, DetectEnv, Jdk};
pub fn default_jdks_dir() -> Option<PathBuf>;
pub fn java_executable(home: &str, windows: bool) -> String;
```

## Used by

- The app (`app/src-tauri/src/commands/java.rs`: `java_jdks`, `java_jdk_inspect`,
  `java_jdk_releases`, `java_jdk_download`, the JDK of JDT LS, runs and `JAVA_HOME`).

## Dependencies

- Workspace crates: [`forge-langservers`](forge-langservers.md) (`managed::Http`, `archive`, `Event`)
- External: `serde`, `serde_json`, `dirs`

## Tests

5 source files, about 1140 lines; 9 tests with fixture JDK folders and a local HTTP server (no
network). `tests/jdtls_smoke.rs` is an ignored end-to-end test against the real JDT LS,
java-debug and, if needed, a Temurin download:

```sh
cargo test -p javaforge-jdk
cargo test -p javaforge-jdk --test jdtls_smoke -- --ignored --nocapture   # network
```
