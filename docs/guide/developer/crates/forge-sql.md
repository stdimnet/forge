---
title: forge-sql
description: "Forge SQL text: dialects, statement splitting, literals, row changes and export (pure, also built to WebAssembly)."
order: 230
tags: [crate, rust]
---

# forge-sql

`crates/forge-sql` — Forge SQL text: dialects, statement splitting, literals, row changes and export (pure, also built to WebAssembly).

## Purpose

Pure SQL text handling shared by `forge-db` and the browser-mode UI (WebAssembly):
dialect quoting, statement splitting and the statement at the caret, cell values as
JavaScript-safe JSON, row change statements and export.

## Modules

| Module | Summary |
| --- | --- |
| `dialect.rs` | SQL dialects: quoting of identifiers and literals. |
| `edit.rs` | Row editing from the data grid: pending inserts, updates and deletes become `INSERT`/`UPDATE`/`DELETE` statements that identify rows by their primary key. |
| `export.rs` | Export a result set as CSV, JSON or SQL `INSERT` statements. |
| `split.rs` | Split a script into statements at `;`, respecting quotes, comments and PostgreSQL dollar quoting. |
| `value.rs` | Cell values as JSON that survives JavaScript: `null`, booleans, numbers only within ±2^53, everything else (big integers, decimals, dates) as strings, binary data as `{"$hex": "89504e47"}`. |

## Main public items

```rust
pub use dialect::Dialect;
pub use edit::{change_sql, ChangeKind, Refused, RowChange};
pub use export::{export, ExportFormat};
pub use split::{leading_keyword, split_statements, statement_at, Statement};
```

## Used by

- IPC: `db_statement_at`, `db_export` (through forge-db)
- UI: `app/ui/src/features/database`
- Browser mode: built into `forge-wasm`

## Dependencies

- Workspace crates: none
- External: `hex`, `serde`, `serde_json`

## Tests

6 source files, about 904 lines; 14 unit tests.

```sh
cargo test -p forge-sql
```
