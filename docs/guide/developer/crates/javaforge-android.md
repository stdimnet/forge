---
title: javaforge-android
description: "JavaForge: Android basics (Gradle Android modules, AndroidManifest.xml, the Android SDK location, adb devices and emulator AVDs, Android App run command lines, Logcat parsing, filtering and stack trace links; pure, also built to WebAssembly)."
order: 276
tags: [crate, rust, pack]
---

# javaforge-android

`packs/javaforge/android` — JavaForge: Android basics (Gradle Android modules, AndroidManifest.xml, the Android SDK location, adb devices and emulator AVDs, Android App run command lines, Logcat parsing, filtering and stack trace links; pure, also built to WebAssembly).

## Purpose

The Android plugin's logic (`forge.android`). Pure Rust (no file system, processes or
devices): the app runs adb and the emulator and hands their output to this crate. See
[Android](../../user/android.md).

- `gradle` recognizes Android modules in build files (AGP plugin ids, version catalog aliases,
  `apply plugin:`; `namespace`, `applicationId`, SDK levels, Kotlin Android) and the AGP version
  (`gradle/libs.versions.toml`, plugin versions, buildscript class paths).
- `manifest` reads `AndroidManifest.xml` with a tolerant scanner (package, `uses-sdk`, label,
  activities and aliases, the `MAIN` / `LAUNCHER` activity), qualifies class names with the
  package or namespace and lists AGP's merged manifest paths.
- `sdk` finds the SDK (setting, `ANDROID_HOME`, `ANDROID_SDK_ROOT`, `local.properties`
  `sdk.dir` with Java properties escapes, the platform's default folder) and its tools; the
  hint shown without one (the SDK is never downloaded).
- `adb` parses `adb devices -l` (states such as `unauthorized` and `no permissions`, model,
  product, transport id, USB port), `emulator -list-avds`, `emu avd name` and `ps`.
- `run` builds the Android App run: `:<module>:assemble<Variant>`, the default APK path of a
  variant (flavors too), `adb install -r -t` and `am start -n` (or `monkey` without an
  activity), chained into one `sh -c` / `cmd /c` command line that echoes each step.
- `logcat` parses `threadtime` lines (also `,year`), tracks the packages of processes (`ps`,
  `Start proc`), marks stack traces and links frames to project sources, implements Android
  Studio's query language (`Query`) and keeps a ring buffer (`Logcat`) that returns filtered
  views.

## Modules

| Module | Summary |
| --- | --- |
| `gradle.rs` | `android_module`, `AndroidModule`, `ModuleKind`, `plugin_kind`, `agp_version`. |
| `manifest.rs` | `parse`, `Manifest`, `Activity`, `qualify`, `merged_manifest_candidates`, `capitalize`. |
| `sdk.rs` | `locate`, `SdkEnv`, `SdkLocation`, `local_properties_sdk_dir`, `default_sdk_dir`, `adb_path`, `emulator_path`, `NO_SDK_HINT`. |
| `adb.rs` | `parse_devices`, `Device`, `parse_avds`, `parse_avd_name`, `parse_ps`. |
| `run.rs` | `AppRun`, `steps`, `chain`, `assemble_task`, `default_apk`. |
| `logcat.rs` | `parse_line`, `LogEntry`, `Level`, `FrameLink`, `frame`, `resolve`, `Query`, `Logcat`, `LogcatView`, `stream_args`, `ps_args`. |

## Main public items

```rust
pub use adb::Device;
pub use logcat::{LogEntry, Logcat, LogcatView, Query};
```

## Used by

- The app: `commands/android.rs` (`android_status`, `android_devices`,
  `android_emulator_start`, the `android_logcat_*` stream and view, Android App command
  lines). [`forge-wasm`](forge-wasm.md): `android_adb`, `android_module`,
  `android_app_command`, `android_logcat_view` for the browser mode.

## Dependencies

- External: `serde`, `serde_json`, `regex-lite`

## Tests

7 source files, about 1800 lines; 14 unit tests on fixtures (`tests/fixtures`: `adb devices
-l`, `ps`, a `threadtime` log with a crash, a manifest, an app build file; no device, no SDK).
