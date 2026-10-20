---
title: phpforge-fw-symfony
description: "PHPForge: Symfony support (service container, routes, Twig templates, console commands)."
order: 300
tags: [crate, rust, pack]
---

# phpforge-fw-symfony

`packs/phpforge/fw-symfony` — PHPForge: Symfony support (service container, routes, Twig templates, console commands).

## Purpose

PHPForge Symfony plugin: everything the IDE knows about a Symfony application.

- `container` reads `config/services.yaml` (explicit services, aliases, `_defaults` and
  autowired `resource:` directories) and, when present, the compiled container XML in
  `var/cache/<env>/`, which also lists the services and parameters of all bundles.
- `routes` collects routes from `#[Route]` attributes, legacy `@Route` annotations and
  YAML route files.
- `twig` lists templates, Twig tags/filters/functions/tests (core, Symfony bridge and
  project extensions) and which controller renders which template.
- `console` finds console commands and builds `bin/console` command lines.
- `complete` answers completion and Go to Declaration for route names, service ids,
  templates, parameters and Twig syntax; `gutter` computes the editor gutter marks.

`scan` builds a `Snapshot` of a project once; the IDE caches it and rescans when files
change.

## Modules

| Module | Summary |
| --- | --- |
| `complete.rs` | Completion and Go to Declaration for Symfony references in PHP, Twig and YAML files: route names (`generateUrl`, `redirectToRoute`, Twig `path()`/`url()`), templates (`render()`, `{% extends %}`), service ids (`@id`, `#[Autowire(service:)]`, `$container->get()`), parameters (`%name%`, `getParameter()`) and Twig tags, filters, functions and tests. |
| `console.rs` | Console commands: the application's own (`#[AsCommand]`, `$defaultName`, `setName()`) plus the common framework and bundle commands, and the `bin/console` command line. |
| `container.rs` | The service container: services and parameters from `config/services*.yaml` (including `resource:` directories registered with autowiring) and from the compiled container XML Symfony dumps to `var/cache/<env>/` in debug mode. |
| `gutter.rs` | Editor gutter marks, like the Symfony plugin's line markers: routes on controller actions, "go to template" next to `render()` calls, "rendered by" on Twig templates and a run marker on console commands. |
| `routes.rs` | Routes from `#[Route]` attributes, legacy `@Route` annotations and YAML route files (`config/routes.yaml`, `config/routes/*.yaml`), including import prefixes. |
| `strings.rs` | String literal helpers shared by completion, navigation and the gutter: find the quoted string around a cursor and the call it is an argument of (`generateUrl('\|')`). |
| `twig.rs` | Twig: templates, the tags/filters/functions/tests available in templates (Twig core, the Symfony bridge and the project's own extensions) and which controllers render which template. |

## Main public items

```rust
pub use complete::{CompletionItem, CompletionKind, CompletionList};
pub use console::ConsoleCommand;
pub use container::{Parameter, Service, ServiceSource};
pub use routes::{Route, RouteSource};
pub use twig::{Template, TwigSymbol, TwigSymbolKind};
pub struct Location { … }
pub struct Config { … }
pub struct SymfonyInfo { … }
pub struct PhpFile { … }
pub struct Snapshot { … }
pub fn is_symfony_project(root: &Path) -> bool { … }
pub fn framework_version(root: &Path) -> Option<String> { … }
pub fn line_col(text: &str, offset: usize) -> (u32, u32) { … }
pub fn index_php_files(root: &Path, dir: &str) -> Vec<PhpFile> { … }
pub fn scan(root: &Path, config: &Config) -> Snapshot { … }
```

## Used by

- IPC: `symfony_detect`, `symfony_refresh`, `symfony_routes`, `symfony_services`, `symfony_parameters`, `symfony_templates`, `symfony_console_commands`, `symfony_complete`, `symfony_goto`, `symfony_gutter`, `symfony_console_run`, `symfony_console_cancel` (`app/src-tauri/src/commands/symfony.rs`)
- UI: `app/ui/src/features/symfony`
- Browser mode: not built to WebAssembly (runtime code); browser mode uses mocks

## Dependencies

- Workspace crates: [`phpforge-lang-php`](phpforge-lang-php.md)
- External: `quick-xml`, `regex`, `serde`, `serde_json`, `walkdir`, `yaml-rust2`

## Tests

8 source files, about 3740 lines; 32 unit tests.

```sh
cargo test -p phpforge-fw-symfony
```
