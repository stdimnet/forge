---
title: forge-sql
description: "Forge console text: dialects, statement splitting, the read-only / production guard, literals, row changes and export (pure, also built to WebAssembly)."
order: 240
tags: [crate, rust]
---

# forge-sql

`crates/forge-sql` — Forge SQL text: dialects, statement splitting, literals, row changes and export (pure, also built to WebAssembly).

## Purpose

Pure console text handling shared by `forge-db` and the browser-mode UI (WebAssembly):
dialect quoting and paging (MySQL, PostgreSQL, SQLite, SQL Server, ClickHouse, Oracle; Redis and
MongoDB consoles), statement splitting and the statement at the caret (`GO` batches, PL/SQL
blocks ending with `/`, one Redis command per line, MongoDB calls), the read-only / production
guard, cell values as JavaScript-safe JSON, row change statements and export.

## Modules

| Module | Summary |
| --- | --- |
| `dialect.rs` | Console dialects: quoting of identifiers and literals, `BEGIN`, paging (`LIMIT`/`OFFSET` or `OFFSET … FETCH`). |
| `edit.rs` | Row editing from the data grid: pending inserts, updates and deletes become `INSERT`/`UPDATE`/`DELETE` statements that identify rows by their key columns (chosen by `forge-dataedit`); `{"$default": true}` sets a column to `DEFAULT`. |
| `export.rs` | Export a result set as CSV, JSON or SQL `INSERT` statements. |
| `guard.rs` | Read-only data sources refuse writes (DML/DDL, Redis write commands, MongoDB write calls); production data sources confirm unfiltered changes (`UPDATE`/`DELETE` without `WHERE`, `TRUNCATE`, `FLUSHDB`, `deleteMany({})`). |
| `split.rs` | Split a script into statements at `;`, respecting quotes, comments, PostgreSQL dollar quoting, T-SQL `GO` and `[brackets]`, Oracle PL/SQL blocks, SQLite `CREATE TRIGGER … BEGIN … END` bodies; Redis lines and MongoDB calls. |
| `value.rs` | Cell values as JSON that survives JavaScript: `null`, booleans, numbers only within ±2^53, everything else (big integers, decimals, dates) as strings, binary data as `{"$hex": "89504e47"}`. |

## Main public items

```rust
pub use dialect::Dialect;
pub use edit::{change_sql, ChangeKind, Refused, RowChange};
pub use export::{export, ExportFormat};
pub use guard::{guard, Guard, GuardPolicy};
pub use split::{leading_keyword, split_statements, statement_at, Statement};
```

## Used by

- IPC: `db_statement_at`, `db_export`, `db_guard` (through forge-db)
- UI: `app/ui/src/features/database`
- Browser mode: built into `forge-wasm` (`sql_split`, `sql_statement_at`, `sql_guard`, …)

## Dependencies

- Workspace crates: none
- External: `hex`, `serde`, `serde_json`

## Tests

7 source files, about 1900 lines; 24 unit tests.

```sh
cargo test -p forge-sql
```
