---
title: Minecraft plugins
description: JavaForge's Minecraft Development support - the Minecraft Plugin project wizard for Paper, Purpur, Folia, Spigot, Bukkit, Velocity and BungeeCord (Gradle Kotlin / Groovy DSL or Maven, Java or Kotlin, run-paper, Shadow, a pinned version table with an optional refresh), plugin.yml / paper-plugin.yml / velocity-plugin.json / bungee.yml completion and validation with a checked main class, the Run Paper Server configuration and event handler gutter icons.
order: 119
tags: [java, javaforge, minecraft, paper, purpur, folia, spigot, bukkit, velocity, bungeecord, plugin.yml, run-paper, gradle, maven]
---

# Minecraft plugins

The **Minecraft Development** plugin (`forge.minecraft`, JavaForge pack) helps you write
server and proxy plugins for Minecraft, in the style of IntelliJ's Minecraft Development
plugin. It builds on [JavaForge](java.md) (Gradle, Maven, JDKs, Eclipse JDT LS) and
[KotlinForge](kotlin.md) for Kotlin sources.

## New Project: Minecraft Plugin

**File | New Project** (`NewProject`) lists **Minecraft Plugin** in the **Java** group. It
creates the project offline, from Forge's own templates: nothing is downloaded while the
project is created.

![The Minecraft Plugin wizard](../../screenshots/minecraft-wizard.png)

| Option | |
| --- | --- |
| Platform | Paper, Purpur, Folia, Spigot, Bukkit (servers); Velocity, BungeeCord / Waterfall (proxies) |
| Descriptor | `plugin.yml` or `paper-plugin.yml` (Paper, Purpur and Folia; Minecraft 1.20.6 and newer) |
| Minecraft | The server version the plugin targets (servers only). Each version shows the Java release it needs |
| Build system | Gradle Kotlin DSL (default), Gradle Groovy DSL or Maven |
| Language | Java or Kotlin (Gradle builds) |
| Group ID | The Maven group and the base package, `com.example` |
| Plugin name | Empty: the main class name derived from the project name (`better-chat` → `BetterChat`) |
| Main class | Empty: `<group id>.<project>.<Project>`, e.g. `com.example.betterchat.BetterChat` |
| Authors, Description, Website | Written into the descriptor (authors are comma-separated) |
| API version | `api-version` of the descriptor; empty: the Minecraft version's (`1.21`) |
| Run Paper server (run-paper) | Applies [run-paper](https://github.com/jpenilla/run-gradle) (`runServer`) and adds the **Run Paper Server** configuration (Gradle builds, servers) |
| Shadow jar | Bundles libraries into the plugin jar (Gradle Shadow, Maven Shade); always on for Kotlin, which bundles its standard library |
| Create Git repository | `git init` |

Options that do not apply to the chosen platform or build are hidden (a proxy has no
Minecraft version or run-paper, Maven has no Kotlin). The preview under the form lists the
files that will be written; problems with the group ID, main class, plugin name or versions
show under the form before you create the project.

The project contains:

- the build: `build.gradle.kts` + `settings.gradle.kts`, `build.gradle` + `settings.gradle`, or
  `pom.xml`, with the platform's repository, its API as a `compileOnly` / `provided`
  dependency, the Java release as toolchain and compiler release, and resource filtering that
  writes the project version into the descriptor (`${version}`, Maven `${project.version}`);
- the descriptor in `src/main/resources`: `plugin.yml` (with the `/hello` command),
  `paper-plugin.yml`, `velocity-plugin.json` or `bungee.yml`; Folia plugins declare
  `folia-supported: true`;
- the main class with `onEnable` / `onDisable` (Velocity: `ProxyInitializeEvent` /
  `ProxyShutdownEvent` with constructor injection), a `/hello` command (Bukkit
  `CommandExecutor`, Paper's Brigadier `BasicCommand` registered through the lifecycle API for
  `paper-plugin.yml`, Velocity `SimpleCommand`, BungeeCord `Command`) and a join listener; Paper
  and proxies use Adventure components, Spigot and Bukkit `ChatColor`;
- `.gitignore` (`build/`, `target/`, `.gradle/`, run-paper's `run/`) and a README;
- with run-paper, `.forge/runConfigurations.toml` with the shared **Run Paper Server**
  configuration.

Gradle projects have no wrapper yet, since the wrapper jar is a binary Forge does not ship: run
`gradle wrapper` once with an installed Gradle (see [Gradle settings and the wrapper](java.md#gradle-settings-and-the-wrapper)).

### Platforms and versions

Forge keeps a pinned table of Minecraft versions (1.16.5 to 1.21.10), the Java release each one
needs (1.16.5: 8, 1.17: 16, 1.18 to 1.20.4: 17, 1.20.5 and newer: 21) and each platform's
dependency:

| Platform | Repository | Dependency |
| --- | --- | --- |
| Paper, Folia | `https://repo.papermc.io/repository/maven-public/` | `io.papermc.paper:paper-api:<version>-R0.1-SNAPSHOT` (1.16.5: `com.destroystokyo.paper:paper-api`) |
| Purpur (1.18+) | Paper's and `https://repo.purpurmc.org/snapshots` | `org.purpurmc.purpur:purpur-api:<version>-R0.1-SNAPSHOT` |
| Spigot | `https://hub.spigotmc.org/nexus/content/repositories/snapshots/` | `org.spigotmc:spigot-api:<version>-R0.1-SNAPSHOT` |
| Bukkit | the Spigot repository | `org.bukkit:bukkit:<version>-R0.1-SNAPSHOT` |
| Velocity | Paper's | `com.velocitypowered:velocity-api:3.4.0-SNAPSHOT` (Java 21) |
| BungeeCord / Waterfall | `https://oss.sonatype.org/content/repositories/snapshots` | `net.md-5:bungeecord-api:1.21-R0.1-SNAPSHOT` (Java 17) |

Folia plugins compile against the Paper API, which contains Folia's region, entity and global
schedulers; Folia (1.20+) only loads plugins that declare `folia-supported: true`.

The **Refresh** button next to **Minecraft** (the version) reads the versions Paper publishes its API
for from `repo.papermc.io` (paper-api's `maven-metadata.xml`) and adds the release versions the
table does not have yet. It only runs when you click it, and the result is cached in the user
cache directory (`<cache>/forge/minecraft/versions.json`), never in a project. New versions
get their coordinates and Java release from the same rules.

Fabric, NeoForge and Forge mods are not generated yet: their templates need Loom or
ModDevGradle with mappings and loader versions that change with every Minecraft release.

## Minecraft plugin projects

A project is a Minecraft plugin when its build (`build.gradle(.kts)`, `pom.xml`,
`gradle/libs.versions.toml`) depends on `paper-api`, `purpur-api`, `folia-api`, `spigot-api`,
`org.bukkit:bukkit`, `velocity-api`, `bungeecord-api` or `waterfall-api`, or uses paperweight's
`paperDevBundle`. The status bar shows the platform and the Minecraft version, for example
**Paper Plugin 1.21.8**, and the plugin activates (see
[project detection](../getting-started/project-detection.md)).

![A generated Paper plugin with the event handler gutter icons](../../screenshots/minecraft-project.png)

### Plugin descriptors

`plugin.yml`, `paper-plugin.yml`, `velocity-plugin.json` and `bungee.yml` in
`src/main/resources` get completion, documentation on hover and validation from schemas
bundled with Forge (see [JSON Schema](json-schema.md)): the keys of each format, `load`,
`api-version`, commands with aliases, usage and permissions, permission defaults and
children, `paper-plugin.yml` dependencies (`load: BEFORE | AFTER | OMIT`, `required`,
`join-classpath`), Velocity's plugin id pattern.

The `main` class is checked against the module's sources (`src/main/java`, `src/main/kotlin`):
an invalid class name or a class that does not exist is an error. **Go to Declaration**
(`Ctrl+B`, `Ctrl+Click`) on the value opens the class.

![plugin.yml with an unresolved main class](../../screenshots/minecraft-plugin-yml.png)

### Run Paper Server

Projects that apply run-paper run their plugin on a real server with the **Run Paper Server**
configuration: a Gradle configuration with the `runServer` task, which builds the plugin,
downloads Paper of the configured version into Gradle's cache and starts it in `run/` with the
plugin installed. Start it from the toolbar, **Tools | Minecraft | Run Paper Server**, Find
Action, or the ▶ gutter icon on the plugin's main class. Generated projects have it as a
shared configuration; in other projects Forge creates a temporary one.

Like every run of project code, Run Paper Server is refused in
[Safe Mode](project-trust.md) until you trust the project. Accept the Minecraft EULA in
`run/eula.txt` on the first start.

### Gutter icons

Methods annotated with `@EventHandler` (Bukkit, Paper, Spigot, BungeeCord) or `@Subscribe`
(Velocity) show an icon with the event they listen to ("Listens to PlayerJoinEvent"). Turn
the icons off in Settings | Languages & Frameworks | Java | **Minecraft** (**Show gutter
icons**).

## Browser mode

`?project=minecraft` opens `better-chat`, a Paper plugin as the wizard generates it plus a
chat listener. The descriptor check, Go to Declaration, the gutter icons and the wizard's
files run in Rust through WebAssembly; **Refresh** answers with canned versions.
