---
title: rubyforge-fw-rails
description: "RubyForge: Ruby on Rails read statically - routes.rb and bin/rails routes output, controllers, views and render references, models with schema.rb / migration columns, database.yml with ERB-lite, locale files and lazy lookup, Stimulus controllers and Turbo frames, editor navigation, completion, gutter marks, inspections and folds, generator lists and Rails command lines (pure, also built to WebAssembly)."
order: 304
tags: [crate, ruby, rails, pack, routes, activerecord, i18n, stimulus, turbo]
---

# rubyforge-fw-rails

`packs/rubyforge/fw-rails` — the analysis behind the [Ruby on Rails](../../user/rails.md)
plugin, in the style of RubyMine. Pure Rust (files in, data out): the desktop backend hands it
the project's files read in one round trip (`commands::rails::project`), browser mode the demo
files through WebAssembly. Nothing of the application runs; `bin/rails routes` and `bin/rails
generate` output (trusted projects only) is passed in as text.

## Purpose

- `source`: a forgiving Ruby scanner — UTF-16 line/column positions, comments, logical lines
  (calls continued with `,`, `\` or open brackets, `=begin`, heredocs), tokens (symbols,
  strings, `%w[]` / `%i[]`, hash keys, `=>`, constants) and call arguments (`Args`:
  positional values, options, `do` blocks, `redirect(…)`).
- `inflect`: ActiveSupport's plural and singular rules with irregular and uncountable words,
  `camelize`, `underscore`, `tableize`, `classify`.
- `routes`: `config/routes.rb` as `bin/rails routes` prints it — `resources` / `resource`
  (options, nesting, `member` / `collection`, `on:`), `namespace`, `scope`, `controller`,
  verbs and `match … via:`, `root`, `mount`, `concern` / `concerns`, `draw`; Rails' names
  (a path's name on its first route, `_index` for uncountable resources) and sample paths;
  `parse_rails_routes` reads the command's table.
- `app`: controllers and mailers with their public actions, views and partials,
  `render` / `layout` references and their files, models (table, `self.table_name`, STI,
  `table_name_prefix`, associations, scopes) linked to their columns.
- `schema`: tables from `db/schema.rb`, then the migrations newer than its version
  (`create_table`, `t.<type>`, `t.references`, `t.timestamps`, `add_column`,
  `remove_column`, `rename_column`, `add_reference`, `change_table`, `rename_table`,
  `drop_table`), with the migrations of each table.
- `yaml`: the YAML of locale files and database.yml (block maps and sequences, scalars, block
  scalars, flow lists, anchors, aliases, `<<:` merges) with key positions.
- `database`: database.yml environments and multi-database entries, ERB-lite for `ENV[…]` /
  `ENV.fetch`, `url:` parsing and Forge's driver per adapter.
- `i18n`: the catalogue of `config/locales/**/*.yml` (plural forms, missing locales), the
  default locale and `t(…)` / `I18n.t(…)` / `translate(…)` calls with lazy lookup.
- `hotwire`: Stimulus controllers (identifier from the path, targets, values, classes,
  outlets, methods), `data-*` attributes of HTML and `data: { … }` hashes, Turbo frame ids.
- `generators`, `command`: built-in and project generators, `bin/rails generate` output,
  and the command lines of `server`, `console`, `dbconsole`, `generate` / `destroy` and
  `routes` from a `RubyContext` (`ruby bin/rails …`, else `bundle exec rails …`).
- `editor`: `goto`, `complete` and `marks` (gutter marks, problems, translation folds) on an
  editor's text, with UTF-16 offsets.

## Modules

| Module | Summary |
| --- | --- |
| `lib.rs` | `Project`, `Options`, `RailsInfo`, `Location`, `wanted`, `is_rails_app`, `gemfile_has_rails`, `rails_version`, `EXTENSIONS`, `FILE_NAMES`. |
| `source.rs` | `LineIndex`, `utf16_to_byte`, `byte_to_utf16`, `logical_lines`, `tokens`, `call_args`, `string_around`, `word_at`. |
| `inflect.rs` | `pluralize`, `singularize`, `camelize`, `underscore`, `tableize`, `classify`. |
| `routes.rs` | `parse_routes`, `parse_rails_routes`, `sample_path`, `Route`. |
| `app.rs` | `controller`, `view`, `model`, `link_models`, `render_refs`, `resolve_render`, `Controller`, `View`, `Model`. |
| `schema.rs` | `tables`, `schema_version`, `migration_version`, `Table`, `Column`. |
| `yaml.rs` | `parse`, `Value`, `Entry`. |
| `database.rs` | `parse`, `render_erb`, `driver`, `DatabaseConfig`. |
| `i18n.rs` | `Catalog`, `default_locale`, `refs`, `lazy_scope`, `TRef`. |
| `hotwire.rs` | `controller`, `identifier`, `stimulus_refs`, `data_attributes`, `frame_refs`, `TURBO_HELPERS`. |
| `generators.rs` | `built_in`, `project_generators`, `parse_generate_output`, `Generator`. |
| `command.rs` | `server_command`, `console_command`, `dbconsole_command`, `generate_command`, `routes_command`, `generators_command`, `RAILS`. |
| `editor.rs` | `goto`, `complete`, `marks`, `enclosing_def`, `Completions`, `FileMarks`. |

Tests run on the `blog` fixture application in `tests/fixtures/blog` (also the browser demo,
`app/ui/src/mock/demoRailsProject.ts`, generated from it).

## Used by

- IPC: `app/src-tauri/src/commands/rails.rs` (`rails_*` commands) and `run.rs` (the `rails`
  run configuration).
- WebAssembly: `crates/forge-wasm/src/rails.rs` (`rails_project`, `rails_goto`,
  `rails_complete`, `rails_marks`, `rails_generators`, `rails_command`).
- Go to Related for Rails files lives in `forge-codeintel` (`related.rs`), project detection
  (`rails`) in `forge-project`.
