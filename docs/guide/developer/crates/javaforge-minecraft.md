---
title: javaforge-minecraft
description: "JavaForge: Minecraft plugins and mods (the New Project generators for Paper, Purpur, Folia, Spigot, Bukkit, Velocity and BungeeCord plugins and Fabric and NeoForge mods with pinned version tables, project detection, plugin descriptor and mod metadata class checks and navigation, neoforge.mods.toml completion, event handler gutter markers; pure, also built to WebAssembly)."
order: 277
tags: [crate, rust, pack]
---

# javaforge-minecraft

`packs/javaforge/minecraft` — JavaForge: Minecraft plugins and mods (the New Project generators for Paper, Purpur, Folia, Spigot, Bukkit, Velocity and BungeeCord plugins and Fabric and NeoForge mods with pinned version tables, project detection, plugin.yml / paper-plugin.yml / velocity-plugin.json / bungee.yml main class checks and navigation, fabric.mod.json and mixin class checks and navigation, neoforge.mods.toml completion, event handler gutter markers; pure, also built to WebAssembly).

## Purpose

The logic of the Minecraft Development plugin (`forge.minecraft`) and of the New Project
wizard's Minecraft Plugin and Minecraft Mod generators. Pure Rust: the app reads files and fetches metadata, this
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
- `mods` holds the mod side: `mods::versions` (the pinned table per Minecraft version: Java,
  Fabric API, Yarn, NeoForge, Parchment; Fabric Loader, Loom, ModDevGradle, Fabric Language
  Kotlin; the daemon JVM file; `resolve_versions`, `mod_version_choices`, and the parsers of
  Fabric's meta API, the Fabric API / NeoForge / plugin `maven-metadata.xml` and NeoForge's
  version scheme), `mods::generate` (`ModOptions` → `ModProject` → files: Gradle build,
  `gradle.properties`, `fabric.mod.json` or `neoforge.mods.toml`, sources from
  `mods::sources` with Mojang or Yarn names per version, assets, PNG textures from
  `mods::png`, run configurations), `mods::editor` (a tolerant JSON scanner, entrypoint and
  mixin references with checks and Go to Declaration), `mods::toml` (`neoforge.mods.toml`
  completion) and `detect_mod` (Loom, ModDevGradle, NeoGradle, ForgeGradle, Quilt Loom).
- `net` (feature `net`, desktop only) fetches the Minecraft versions from Paper's repository
  and, with `refresh_mods`, the mod versions (Fabric meta, Fabric API, Loom, NeoForge,
  ModDevGradle); it is only called when the user clicks Refresh.

## Modules

| Module | Summary |
| --- | --- |
| `versions.rs` | `Platform`, `MC_VERSIONS`, `java_for`, `api_version`, `api_spec`, `ApiSpec`, `check_platform`, `supports_paper_plugin_yml`, `mc_versions_from_paper_metadata`, `version_choices`, `compare`. |
| `generate.rs` | `Options`, `resolve`, `Project`, `generate`, `files`, `GeneratedFile`, `Build`, `Descriptor`, `class_name`, `package_segment`, `velocity_id`. |
| `sources.rs` | `Flavor`, `java`, `kotlin` (source templates). |
| `detect.rs` | `detect`, `Detected`. |
| `descriptor.rs` | `kind`, `main_entry`, `module_prefix`, `class_candidates`, `inspect`, `Problem`, `goto`, `Target`, `declaration_line`. |
| `markers.rs` | `markers`, `Marker`, `MarkerKind`. |
| `mods/versions.rs` | `Loader`, `MOD_VERSIONS`, `ModVersions`, `remember`, `resolve_versions`, `resolve_with`, `mod_version_choices`, `neoforge_mc_version`, `fabric_loader_from_meta`, `game_versions_from_meta`, `yarn_from_meta`, `fabric_api_from_metadata`, `neoforge_from_metadata`, `newest_release`. |
| `mods/generate.rs` | `ModOptions`, `resolve`, `ModProject`, `Mappings`, `Environment`, `generate_mod`, `mod_files`, `mod_id_of`, `is_mod_id`. |
| `mods/sources.rs`, `mods/png.rs` | Source templates and the `Api` names (Mojang / Yarn); the PNG encoder and placeholder textures. |
| `mods/editor.rs` | `kind`, `json_strings`, `references`, `inspect`, `goto`, `class_candidates`. |
| `mods/toml.rs` | `is_mods_toml`, `complete`, `TomlCompletion`. |
| `mods/mod.rs` | `detect_mod`, `DetectedMod`. |
| `net.rs` | `refresh`, `fetch_mc_versions`, `default_repositories`, `refresh_mods`, `ModSources` (feature `net`). |

## Used by

- [`forge-project`](forge-project.md): the `minecraft-plugin` and `minecraft-mod` generators
  (`templates::render`) and the `minecraft` framework of project detection (plugins and mods).
- The app: `commands/minecraft.rs` (`minecraft_versions_refresh`, `minecraft_inspect`,
  `minecraft_goto`, `minecraft_complete`, `minecraft_gutter`), `project_generators_list` with
  refreshed versions.
- [`forge-wasm`](forge-wasm.md): `minecraft_generate`, `minecraft_version_choices`,
  `minecraft_inspect`, `minecraft_goto`, `minecraft_complete`, `minecraft_gutter` for the
  browser mode.

## Dependencies

- External: `serde`, `serde_json`; `reqwest` with the `net` feature.

## Tests

Unit tests for versions, validation, every platform / build / language combination,
detection of generated builds, descriptors and markers; golden files of six typical projects
(`tests/golden/*.txt`, regenerate with `FORGE_UPDATE_SNAPSHOTS=1 cargo test -p
javaforge-minecraft --test golden`); "Refresh versions" against a local HTTP server with a
`maven-metadata.xml` fixture. Mods: the version table's consistency, the refresh parsers on
trimmed real answers (`tests/fixtures/fabric-meta-*.json`, `fabric-api-`, `neoforge-`,
`moddev-maven-metadata.xml`), `refresh_mods` against a local server, validation, every
loader / version / language / environment / mappings combination (valid JSON, only the
properties Gradle expands), the PNG encoder, the editor checks and TOML completion, and golden
files of five mods (`tests/golden/mod-*.txt`). `FORGE_MC_MOD_EXPORT=<dir> cargo test -p
javaforge-minecraft --test golden export` writes generated mods for a manual Gradle build. No
network.
