---
title: javaforge-minecraft
description: "JavaForge: Minecraft plugins (the New Project generator for Paper, Purpur, Folia, Spigot, Bukkit, Velocity and BungeeCord with a pinned version table, project detection, plugin descriptor main class checks and navigation, event handler gutter markers; pure, also built to WebAssembly)."
order: 277
tags: [crate, rust, pack]
---

# javaforge-minecraft

`packs/javaforge/minecraft` — JavaForge: Minecraft plugins (the New Project generator for Paper, Purpur, Folia, Spigot, Bukkit, Velocity and BungeeCord with a pinned version table, project detection, plugin.yml / paper-plugin.yml / velocity-plugin.json / bungee.yml main class checks and navigation, event handler gutter markers; pure, also built to WebAssembly).

## Purpose

The logic of the Minecraft Development plugin (`forge.minecraft`) and of the New Project
wizard's Minecraft Plugin generator. Pure Rust: the app reads files and fetches metadata, this
crate decides. See [Minecraft plugins](../../user/minecraft.md).

- `versions` is the pinned table: Minecraft versions with their Java release, the rules for
  versions outside the table, each platform's repositories and API coordinates
  (`api_spec`), lower bounds (Purpur 1.18, Folia 1.20, `paper-plugin.yml` 1.20.6), the plugin
  versions of generated builds (run-paper, Shadow, Kotlin, Maven plugins) and the parsing of
  paper-api's `maven-metadata.xml` for "Refresh versions".
- `generate` validates the wizard's `Options` (`resolve` → `Project`: names, package, main
  class, descriptor, Java release) and writes the files: Gradle Kotlin / Groovy DSL or Maven
  builds, the descriptor, `.gitignore`, README and the shared "Run Paper Server"
  configuration. `sources` holds the Java and Kotlin templates of the main class, `/hello`
  command and join listener per API flavor (Paper, `paper-plugin.yml` with Brigadier,
  Spigot / Bukkit, Velocity, BungeeCord).
- `detect` recognizes a Minecraft plugin from build texts (API artifact, Minecraft version,
  run-paper).
- `descriptor` finds the `main` entry of a descriptor (UTF-16 offsets), checks it against the
  module's sources and resolves Go to Declaration.
- `markers` scans Java / Kotlin sources for `@EventHandler` / `@Subscribe` methods (with the
  event type) and `JavaPlugin` main classes.
- `net` (feature `net`, desktop only) fetches the Minecraft versions from Paper's repository;
  it is only called when the user clicks Refresh.

## Modules

| Module | Summary |
| --- | --- |
| `versions.rs` | `Platform`, `MC_VERSIONS`, `java_for`, `api_version`, `api_spec`, `ApiSpec`, `check_platform`, `supports_paper_plugin_yml`, `mc_versions_from_paper_metadata`, `version_choices`, `compare`. |
| `generate.rs` | `Options`, `resolve`, `Project`, `generate`, `files`, `GeneratedFile`, `Build`, `Descriptor`, `class_name`, `package_segment`, `velocity_id`. |
| `sources.rs` | `Flavor`, `java`, `kotlin` (source templates). |
| `detect.rs` | `detect`, `Detected`. |
| `descriptor.rs` | `kind`, `main_entry`, `module_prefix`, `class_candidates`, `inspect`, `Problem`, `goto`, `Target`, `declaration_line`. |
| `markers.rs` | `markers`, `Marker`, `MarkerKind`. |
| `net.rs` | `fetch_mc_versions`, `default_repository` (feature `net`). |

## Used by

- [`forge-project`](forge-project.md): the `minecraft-plugin` generator (`templates::render`)
  and the `minecraft` framework of project detection.
- The app: `commands/minecraft.rs` (`minecraft_versions_refresh`, `minecraft_inspect`,
  `minecraft_goto`, `minecraft_gutter`), `project_generators_list` with refreshed versions.
- [`forge-wasm`](forge-wasm.md): `minecraft_generate`, `minecraft_version_choices`,
  `minecraft_inspect`, `minecraft_goto`, `minecraft_gutter` for the browser mode.

## Dependencies

- External: `serde`, `serde_json`; `reqwest` with the `net` feature.

## Tests

Unit tests for versions, validation, every platform / build / language combination,
detection of generated builds, descriptors and markers; golden files of six typical projects
(`tests/golden/*.txt`, regenerate with `FORGE_UPDATE_SNAPSHOTS=1 cargo test -p
javaforge-minecraft --test golden`); "Refresh versions" against a local HTTP server with a
`maven-metadata.xml` fixture. No network.
