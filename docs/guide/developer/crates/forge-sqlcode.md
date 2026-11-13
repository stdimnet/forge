---
title: forge-sqlcode
description: "The SQL editor (pure, also built to WebAssembly): dialects per file, resolve against the data source model, the SQL inspections and quick fixes, the formatter, user parameters, query history and Run SQL Script planning."
order: 241
tags: [crate, rust, sql]
---

# forge-sqlcode

`crates/forge-sqlcode` — the SQL editor of Forge, modelled on DataGrip (pure; also built to
WebAssembly for browser mode). User documentation: [SQL editor](../../user/sql-editor.md).

## Purpose

Everything the SQL editor computes: which dialect a file has, what each name of a script is
(a table, column, schema or routine of the data source model, or an alias, CTE or subquery of
the script), the SQL inspections with their quick fixes, Go to Declaration / Find Usages /
alias rename, the formatter behind Reformat Code and the SQL code style preview, user
parameters, the query history model and the statements Run SQL Script executes. It works on
an error-tolerant tokenizer, so half-typed queries still resolve; it never connects to a
database (the app passes the cached `DbModel`).

## Modules

| Module | Summary |
| --- | --- |
| `lang.rs` | `SqlDialect` (generic, MySQL, PostgreSQL, SQLite, SQL Server, Oracle, ClickHouse): lexer quirks, keywords, types; `DialectMapping`, `dialect_for` and `data_source_for` (most specific path wins). |
| `lex.rs` | Tokens with byte offsets: words, quoted identifiers (`"…"`, `` `…` ``, `[…]`), strings (`E''`, `N''`, `$$`, `$tag$`), numbers, comments (`--`, `/* */`, `#`), parameters, variables, operators. |
| `catalog.rs` | The model as the resolver sees it: tables, schemas, routines and sequences by name, with `Lookup::Unknown` for schemas that were not introspected. |
| `resolve.rs` | `analyze(text, dialect, model, default_schema) -> Analysis`: statements split like the console, then scopes per query (FROM items with aliases, CTEs, subqueries, table functions, `USING` / `NATURAL`), references with `Target`s and `State`s, definitions, and findings (without `WHERE`, redundant code, type mismatches). |
| `inspect.rs` | `inspect(&Analysis) -> Vec<Problem>`: the inspections `SqlResolve`, `SqlAmbiguousColumn`, `SqlWithoutWhere`, `SqlUnusedAlias`, `SqlUnusedCte`, `SqlType`, `SqlRedundantOrderingDirection`, `SqlRedundantElseNull`, `SqlRedundantAlias` with their fixes (edits and the selection after them). |
| `navigate.rs` | `symbol_at`, `occurrences` (Find Usages in the file), `rename_ranges` (aliases, CTEs and select-list aliases only) and `ddl_location` (the `CREATE TABLE` / `ALTER TABLE … ADD` of an object in a DDL data source's files). |
| `format.rs` | `SqlStyle` (from the code style values `sql.*`) and `format(text, dialect, style)`: keyword / identifier case, clause lines, river alignment, list items, comma position, AND / OR lines, indented subqueries and column lists, spaces, terminators and comments between statements; routines keep their layout. `change_case` is the case-only pass. |
| `params.rs` | `ParamPatterns`, `find_params`, `param_names`, `substitute`. |
| `history.rs` | `History` (newest first, consecutive repeats merged, capped), `HistoryQuery` search and the per-data-source file name. |
| `api.rs` | Requests and answers of the IPC commands and the WebAssembly bridge with UTF-16 offsets (`Offsets`), `plan_run` for Run SQL Script. |

## Main public items

```rust
pub use format::{format, SqlStyle};
pub use lang::{data_source_for, dialect_for, DialectMapping, SqlDialect};
pub use params::ParamPatterns;
// api::{analyze_text, declaration, usages, rename, params, plan_run, SqlContext}
// resolve::{analyze, Analysis, Ref, Def, Target, Role, State}
```

## Used by

- IPC (`app/src-tauri/src/commands/sql.rs`): `sql_dialects`, `sql_file_info`, `sql_analyze`,
  `sql_declaration`, `sql_usages`, `sql_rename`, `sql_format`, `sql_params`, `sql_substitute`,
  `sql_history_list` / `_add` / `_clear`, `sql_run_file` (events `sql://runFile`) and
  `sql_run_file_cancel`.
- `forge-codestyle`: the SQL preview of Settings | Editor | Code Style | SQL.
- Browser mode: `forge-wasm` (`sql_*` functions, `app/ui/src/mock/sqlMock.ts`).
- UI: `app/ui/src/features/sqlEditor` and the console in `features/database`.

## Dependencies

- Workspace crates: `forge-sql` (statement splitting, console dialects), `forge-dbmodel`
  (the model types, DDL files)
- External: `serde`, `serde_json`

## Tests

13 source files, about 6100 lines; unit tests for the lexer, dialect mappings, resolve, every
inspection and its fix, parameters, history and offsets, plus the formatter's fixture corpus
(`tests/format/*.sql`: input, `-- @expected` output, `-- @dialect` / `-- @style` headers; each
expected output must be stable when formatted again; `FORGE_UPDATE_SNAPSHOTS=1` rewrites them).

```sh
cargo test -p forge-sqlcode
```
