---
title: rubyforge-lang-ruby
description: "RubyForge: Ruby language support - version files, Gemfile and Gemfile.lock, Rake tasks from rake --tasks and Rakefiles, RSpec and Minitest discovery and the test provider of the shared test runner, Ruby / Rake / Bundler / gem / IRB / rdbg command lines, RuboCop JSON reports and autocorrect (pure, also built to WebAssembly)."
order: 302
tags: [crate, ruby, pack, bundler, rake, rspec, minitest, rubocop]
---

# rubyforge-lang-ruby

`packs/rubyforge/lang-ruby` — RubyForge's Ruby language support, in the style of RubyMine.
Pure Rust (no I/O), shared by the desktop backend and the WebAssembly build of browser mode.

## Purpose

- `version`: the Ruby a project asks for — `.ruby-version`, `.tool-versions`,
  `mise.toml` / `.mise.toml`, the Gemfile's `ruby "3.3.6"` or `ruby file: ".ruby-version"`, in
  that order — version parts, engines (`jruby-9.4`) and matching (`3.3` matches `3.3.6`).
- `gemfile`: a tolerant Gemfile scanner (sources, `ruby`, `gemspec`, `gem` with requirements,
  groups from blocks and options, platforms, `require: false`, `path:` / `git:` / `github:`)
  and the Gemfile.lock sections (`GEM`, `GIT`, `PATH` specs with dependencies, `PLATFORMS`,
  `DEPENDENCIES`, `RUBY VERSION`, `BUNDLED WITH`); `bundle` merges both into `Bundle` (direct
  and transitive gems with groups, missing gems).
- `rake`: `rake --tasks --all` output (tasks, arguments, descriptions), `rake --where`
  locations, and `scan_rakefile` for `desc` / `task` / `namespace` and the tasks of
  Bundler's gem tasks, `RSpec::Core::RakeTask`, `Rake::TestTask`, `Minitest::TestTask` and
  `RuboCop::RakeTask` without running Rake.
- `discovery`: RSpec groups and examples with their full descriptions, Minitest test classes,
  `def test_*` methods, ActiveSupport `test "…"` blocks and Minitest::Spec blocks
  (`test_0001_<description>`), nested by indentation.
- `provider`: `RubyTests`, the `forge-testing` provider of the `rspec` and `minitest`
  manifest frameworks — gutter targets keyed like the test tree's results, `rspec
  file:line … --format json --out <report dir>/rspec.json` and `ruby -Ilib -Itest <file> -v
  -n <filter>` (or a loader for folders) through `bundle exec`, Rerun Failed Tests.
- `command`: Ruby script and Rake configurations, `rake --tasks`, Bundler actions, `gem
  install`, the IRB console and `rdbg --open --host 127.0.0.1 --port ${port} --command -- …`.
- `rubocop`: `rubocop --format json --stdin <path>` arguments, the JSON report as
  `RubyProblem`s (0-based positions, RuboCop's levels as Forge's), autocorrect arguments
  (`-a` / `-A` with `--stderr`) and the corrected source.
- `context`: `RubyContext` (interpreter, `bundle`, `use_bundler`, gem executables,
  environment, the bundle's gems), handed to the provider as `CommandContext.settings["ruby.context"]`.

## Modules

| Module | Summary |
| --- | --- |
| `version.rs` | `requested_version`, `parse_ruby_version`, `parse_tool_versions`, `parse_mise_toml`, `parse_gemfile_ruby`, `matches`, `compare`, `engine`. |
| `gemfile.rs` | `parse_gemfile`, `parse_lockfile`, `bundle`, `Gemfile`, `Lockfile`, `Bundle`, `BundleGem`. |
| `rake.rs` | `parse_tasks`, `merge_locations`, `scan_rakefile`, `RakeTask`. |
| `discovery.rs` | `discover_rspec`, `discover_minitest`, `is_spec_file`, `is_test_file`, `RubyTest`. |
| `provider.rs` | `RubyTests`, `register`, `rspec_command`, `minitest_command`, `RSPEC`, `MINITEST`. |
| `command.rs` | `script_command`, `rake_command`, `rake_tasks_command`, `bundler_action`, `gem_install`, `irb_command`, `rdbg_command`, `RUBY`, `RAKE`. |
| `rubocop.rs` | `check_args`, `autocorrect_args`, `parse_report`, `corrected_source`, `RubyProblem`. |
| `context.rs` | `RubyContext`, `RubyCommand`, `CONTEXT_KEY`. |

## Used by

- IPC: `app/src-tauri/src/commands/ruby.rs` and `run.rs` (`RubyTests` in `RunState::new`).
- WebAssembly: `crates/forge-wasm/src/ruby.rs` (`ruby_bundle`, `ruby_rake_tasks`,
  `ruby_command_line`, `ruby_action_command`, `ruby_script_entry`, `ruby_rubocop_report`,
  `ruby_requested_version`) and the test registry of `testing.rs`.
- [`rubyforge-sdk`](rubyforge-sdk.md).

## Dependencies

- Workspace crates: [`forge-output`](forge-output.md), [`forge-runtools`](forge-runtools.md),
  [`forge-testing`](forge-testing.md)
- External: `serde`, `serde_json`

## Tests

Unit tests on fixtures (`tests/fixtures`: a Gemfile and Gemfile.lock, a real `rake --tasks
--all` output, a RuboCop JSON report). An ignored smoke test runs Minitest through the
provider's command lines with a real Ruby and parses the output with `forge-output`.

```sh
cargo test -p rubyforge-lang-ruby
cargo test -p rubyforge-lang-ruby -- --ignored   # needs ruby with minitest
```
