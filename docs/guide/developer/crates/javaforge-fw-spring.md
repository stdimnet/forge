---
title: javaforge-fw-spring
description: "JavaForge: Spring Boot and Jakarta EE (Java / Kotlin declaration scanner, beans and injection points, Spring MVC and JAX-RS endpoints, JPA entities, application.properties / application.yml assistance from spring-configuration-metadata.json and the dependency jars, the Spring Boot run configuration, actuator client and run dashboard; pure, also built to WebAssembly)."
order: 273
tags: [crate, rust, pack, spring]
---

# javaforge-fw-spring

`packs/javaforge/fw-spring` — JavaForge: Spring Boot and Jakarta EE (Java / Kotlin declaration
scanner, beans and injection points, Spring MVC and JAX-RS endpoints, JPA entities,
application.properties / application.yml assistance from spring-configuration-metadata.json and
the dependency jars, the Spring Boot run configuration, actuator client and run dashboard; pure,
also built to WebAssembly).

## Purpose

The logic of the plugin `forge.spring`; see [Spring Boot and Jakarta EE](../../user/spring.md).
Everything is static analysis of the texts the caller hands in: the application is never
started to find beans or endpoints. Two features add I/O for the desktop: `jars` (reading the
dependency jars as zip files) and `net` (a plain HTTP/1.1 client for the actuator on
localhost). Without them the crate builds to WebAssembly.

- `scan` lexes Java and Kotlin (comments, text blocks, raw strings, string templates, backtick
  names) and reads package, imports, classes (kind, supertypes with type arguments, enum
  constants), constructors (Kotlin primary constructors with `val` / `var` properties),
  methods, fields and parameters with their annotations **and annotation arguments** (string
  literals, concatenations, constants, class literals, arrays), doc comments, literal
  defaults and string constants (also `API + "/stock"` across files). Half-typed code still
  yields the declarations around it.
- `model` builds the project model: beans (stereotypes and custom meta-annotated ones, `@Bean`
  methods, Spring Data repositories, `@ConfigurationProperties`, CDI scopes, EJB, JAX-RS
  resources, `@Produces`), injection points (constructors, Lombok's required-args fields,
  `@Autowired` / `@Inject` / `@Resource` fields and setters, `@Bean` parameters) matched by
  type and supertypes with qualifiers, `@Primary` and name fallback; Spring MVC endpoints with
  class prefixes, methods, media types and parameters, JAX-RS endpoints below the
  `@ApplicationPath`; JPA entities with columns, ids and relations linked to their
  repositories; `@ConfigurationProperties` classes turned into metadata; the APIs in use
  (`jakarta.*`, `javax.*`, Spring); profiles; configuration values per profile; and the gutter
  markers of a file. `Project` keeps the scanned files so an edited file is scanned again on its
  own.
- `config` reads `application*.properties` / `application*.yml` (documents, lists, block
  scalars, quoted keys, continuation lines) into dotted keys with byte ranges, finds where the
  cursor is (a key being typed with its YAML parent path, or a value), and implements relaxed
  binding (`canonical`).
- `metadata` holds `spring-configuration-metadata.json` (properties, groups, hints, deprecations)
  plus enum constants; `assist` completes keys (nested YAML insertion) and values (booleans,
  enums, hints, profiles) with Markdown documentation, inspects unknown / deprecated keys and
  invalid values (with the replacement text of the quick fix), hovers and navigates to
  `@ConfigurationProperties`.
- `jars` (feature `jars`) finds the jars of the synced coordinates in `~/.m2/repository` and
  Gradle's `modules-2/files-2.1` cache (or Spring Boot's own jars of the build's version, else
  the newest cached), reads their metadata files and the enum constants of property types from
  class files (`class_enum_constants`).
- `run` turns the `spring-boot` configuration into an Application run (profiles, colored
  output, `--key=value` overrides, actuator exposure) and follows the console (`Observed`:
  port, context path, server, profiles, PID, startup time, failures, Spring Boot version).
- `actuator` parses `/actuator/health`, `/beans` and `/mappings` (Spring Boot 2 and 3, MVC and
  WebFlux); with `net`, `probe` reads them from `127.0.0.1` (chunked and length-delimited
  bodies, 503 health).
- `dashboard` builds the Services nodes and details of the run dashboard; `views` merges the
  model with a running application's beans and mappings and writes "Open in HTTP Client"
  requests.

## Modules

| Module | Summary |
| --- | --- |
| `scan.rs` | `parse(path, text) -> SourceFile`, `Lang`, `ClassInfo`, `Callable`, `Field`, `Param`, `Annotation`, `AnnArg`, `Val`, `TypeRef`, `resolve_constants`. |
| `model.rs` | `analyze(files) -> Model`, `Project`, `Bean`, `Injection`, `Endpoint`, `Entity`, `ConfigClass`, `Application`, `Api`, `gutter(model, path) -> Marker[]`, `setting(model, key, profiles)`, `injected_type`, `join_paths`. |
| `config.rs` | `config_file`, `Format`, `parse`, `Entry`, `place_at`, `Place`, `canonical`, `kebab`. |
| `metadata.rs` | `Metadata`, `Property`, `Hint`, `Group`, `Deprecation`, `short_type`. |
| `assist.rs` | `complete`, `inspect`, `hover`, `goto`, `documentation`, `Context`, `Completion`, `Problem`, `Hover`. |
| `jars.rs` | `Caches`, `Coordinate`, `jars_for`, `jar_metadata`, `load`, `enum_constants`, `class_enum_constants` (feature `jars`). |
| `run.rs` | `TYPE`, `launch`, `Launch`, `profiles`, `overrides`, `Observed`, `strip_ansi`. |
| `actuator.rs` | `parse_health`, `parse_beans`, `parse_mappings`, `Snapshot`; `get`, `probe` (feature `net`). |
| `dashboard.rs` | `DashboardApp`, `tree`, `details`, `node_id`, `app_name`. |
| `views.rs` | `EndpointRow`, `BeanRow`, `endpoint_rows`, `bean_rows`, `http_request`. |

## Used by

- The app: `commands/spring.rs` (the `spring_*` IPC commands: model cache, jar metadata cache,
  run tracking and actuator probes) and `commands/run.rs` (Spring Boot runs become Application
  runs; their output goes to the dashboard's observer).
- [`forge-wasm`](forge-wasm.md): `spring_overview`, `spring_gutter`, `spring_config`,
  `spring_http_request`, `spring_launch`, `spring_observe`, `spring_dashboard` for the browser
  mode (`?project=spring`).
- [`forge-project`](forge-project.md) detects the `spring-boot` and `jakarta-ee` frameworks
  that activate the plugin.

## Dependencies

- External: `serde`, `serde_json`; `zip` (feature `jars`).

## Tests

12 source files; unit tests on inline Java / Kotlin / YAML / properties fixtures, a fake
metadata JSON, jars and class files written by the tests, and a local mock HTTP server for the
actuator (no network).
