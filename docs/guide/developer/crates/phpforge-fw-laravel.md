---
title: phpforge-fw-laravel
description: "PHPForge: Laravel support (routes, views, Blade components, config/env, Eloquent, translations, artisan)."
order: 300
tags: [crate, rust, pack]
---

# phpforge-fw-laravel

`packs/phpforge/fw-laravel` — PHPForge: Laravel support (routes, views, Blade components, config/env, Eloquent, translations, artisan).

## Purpose

PHPForge Laravel support, built on the PHP pack's tree-sitter parser.

`Laravel::load` analyses a project statically:
- `routes`: `routes/*.php` (groups, prefixes, names, middleware, resources) or the
  output of `php artisan route:list --json`;
- `views`: view names, Blade components (anonymous, class based, Livewire) and custom
  Blade directives;
- `config`: `config()` and `env()` keys;
- `eloquent`: models with columns from migrations, `$fillable`, casts, relations,
  scopes and accessors;
- `translations`: `__()` keys from PHP and JSON language files.

On top of that, `complete` offers completion, Go to Declaration and gutter markers
for an editor buffer, and `artisan` lists and builds artisan commands.

## Modules

| Module | Summary |
| --- | --- |
| `artisan.rs` | Artisan commands: the list from `php artisan list --format=json` (with a built-in fallback when PHP is not available), command line splitting, and running commands with streamed output. |
| `complete.rs` | Editor services for PHP and Blade buffers: completion of route names, views, config and env keys, translations, columns, relations, model properties, scopes, components and custom directives; Go to Declaration for the same; gutter markers for routed controller methods, route definitions and `view()` calls. |
| `config.rs` | `config('app.name')` keys from `config/*.php` and `env('APP_NAME')` keys from `.env` files. |
| `context.rs` | What the caret is on, found lexically so it works while the code is being typed (`route('orders.\|` has no closing quote yet): a string argument of a call, a property access, a Blade component tag or a Blade directive. |
| `eloquent.rs` | Eloquent models: table columns from migrations, `$fillable`, casts, relations (with the related model), local scopes, accessors and `@property` docblocks. |
| `php.rs` | Small helpers over the tree-sitter PHP syntax tree: literal values, call arguments and returned arrays. Laravel's configuration lives in plain PHP arrays and fluent calls, so static evaluation of these few shapes covers routes, config, translations and models. |
| `project.rs` | Project layout: Laravel version, PSR-4 autoloading, file helpers and locations. |
| `routes.rs` | Routes: static parsing of `routes/*.php` (verbs, `match`, `resource`, `view`, `redirect`, groups with prefix/name/middleware/controller, included route files) and the JSON output of `php artisan route:list --json`. |
| `translations.rs` | Translation keys for `__()`, `trans()`, `trans_choice()` and `@lang`: grouped PHP files (`lang/en/messages.php` → `messages.welcome`) and JSON files (`lang/de.json`). |
| `views.rs` | Views and Blade components: `view('orders.show')` names, anonymous components in `resources/views/components`, class components in `app/View/Components`, Livewire components and custom directives registered with `Blade::directive` / `Blade::if`. |

## Main public items

```rust
pub use complete::{CompletionItem, Completions, GutterMark};
pub use config::KeyEntry;
pub use eloquent::{Model, ModelAttribute, Relation, Scope};
pub use project::{Location, Psr4};
pub use routes::Route;
pub use translations::Translation;
pub use views::{Component, CustomDirective, View};
pub struct Laravel { … }
pub struct LaravelInfo { … }
pub use project::is_laravel;
pub fn fingerprint(root: &Path) -> u64 { … }
```

## Used by

- IPC: `laravel_detect`, `laravel_routes`, `laravel_complete`, `laravel_goto`, `laravel_gutter`, `laravel_artisan_commands`, `laravel_artisan_run`, `laravel_artisan_cancel` (`app/src-tauri/src/commands/laravel.rs`)
- UI: `app/ui/src/features/laravel`
- Browser mode: not built to WebAssembly (runtime code); browser mode uses mocks

## Dependencies

- Workspace crates: [`phpforge-lang-php`](phpforge-lang-php.md)
- External: `serde`, `serde_json`, `thiserror`, `tracing`, `tree-sitter`, `walkdir`

## Tests

11 source files, about 4735 lines; 18 unit tests and 11 integration tests in `tests/` (`fixture.rs`).

```sh
cargo test -p phpforge-fw-laravel
```
