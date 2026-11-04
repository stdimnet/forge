---
title: forge-i18n
description: "Localization: ICU MessageFormat catalogs, locale fallback, plural rules and number/date formatting (pure, WebAssembly-friendly)."
order: 119
tags: [crate, rust, i18n, localization]
---

# forge-i18n

`crates/forge-i18n` — Localization: ICU MessageFormat catalogs, locale fallback, plural rules and number/date formatting (pure, WebAssembly-friendly).

## Purpose

- Parses and formats ICU MessageFormat messages: arguments, `plural` (with `=n`, `offset:`
  and `#`), `selectordinal`, `select`, `number` (`integer`, `percent`), `date` and `time`
  (`short`, `medium`, `long`) and apostrophe quoting.
- Bundles the catalogs `locales/<locale>/<area>.json` (English source, German translation)
  and looks keys up along the fallback chain `de-AT` → `de` → `en`. A key missing in a
  language falls back to English, never to the key.
- CLDR plural rules (cardinal and ordinal) for English, German and the common European and
  Asian languages; decimal and grouping separators and date patterns per language.
- Checks translations against the source: unknown keys, syntax errors and argument
  mismatches (the key-parity test), plus coverage.
- A process-wide localizer (`set_locale`, `t`, `t_args`, `t_or`) for Rust-originated text:
  the main menu and action titles (`forge-keymap`), plugin manifests (`forge-plugin-api`),
  Settings group titles, Safe Mode refusals (`forge-trust`) and the `ForgeError` messages
  sent to the UI (`forge-core`).

See the [Localization](../localization.md) guide for the key naming and how to add strings
and languages.

## Modules

| Module | Summary |
| --- | --- |
| `message.rs` | ICU MessageFormat parser and formatter, argument names of a message. |
| `plural.rs` | CLDR cardinal and ordinal plural categories per language. |
| `format.rs` | Number formatting (grouping, percent) and dates and times per language. |
| `catalog.rs` | Bundled catalog files, supported locales, translation checks, coverage and key slugs. |
| `localizer.rs` | `Localizer` (lookup with fallback, `t`, `t_args`, `t_or`, `translate`) and the process-wide instance. |

## Main public items

```rust
pub use catalog::{builtin_messages, check_translation, coverage, locales, slug, Coverage, LocaleInfo, Problem, SOURCE_LOCALE};
pub use localizer::{add_messages, current, normalize_locale, set_locale, set_timezone_offset, t, t_args, t_or, Localizer};
pub use message::{Args, Message, ParseError};
```

## Used by

- `forge-keymap` (menu labels, action titles), `forge-plugin-api` (`l10n` module,
  Settings groups), `forge-trust` (Safe Mode refusals), `forge-app` (sets the language from
  `ide.language`, localizes `plugins_list` and `settings_schema`).
- UI: `app/ui/src/i18n` (`t()`, `<T>`) through the `i18n_*` functions of `forge-wasm`.
- Browser mode: built into `forge-wasm`.

## Dependencies

- Workspace crates: none
- External: `serde`, `serde_json`

## Tests

The catalogs parse, every file is listed, no key is defined twice, German matches English
(keys and arguments) and the hand-written areas are fully translated; messages, plurals,
numbers, dates and fallback have unit tests.

```sh
cargo test -p forge-i18n
```
