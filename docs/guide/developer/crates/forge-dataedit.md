---
title: forge-dataedit
description: "Forge data editor logic (pure, also built to WebAssembly): server-side sort and filter SQL per dialect, local sort and filter, row keys and DML preview, foreign-key navigation, aggregates, data compare, chart series and the value editor."
order: 62
tags: [crate, rust, database, data editor]
---

# forge-dataedit

`crates/forge-dataedit` — the logic of the database [data editor](../../user/database.md#data-editor)
(DataGrip's data editor), pure and also built to WebAssembly so the desktop backend and the
browser-mode mock answer the same way.

## Purpose

- Header sort and the filter row become `WHERE` / `ORDER BY` per dialect. The typed condition is
  checked first: one condition (no `;`, balanced brackets, closed quotes and comments), no
  write keywords at any depth (also inside subqueries) and nothing the read-only guard of
  `forge-sql` blocks.
- Result sets of arbitrary queries are sorted and filtered in memory instead.
- Pending changes become DML that identifies rows by the primary key, else a unique key whose
  columns are all `NOT NULL`; tables without one refuse updates and deletes.
- Foreign-key navigation (real and virtual keys) in both directions, aggregates of selected
  cells (exact decimal sums), the diff of two result sets by key, chart series and the value
  editor (editor kind, JSON/XML formatting with validation, hex dumps, image detection, dates).

## Modules

| Module | Summary |
| --- | --- |
| `query.rs` | `TableQuery` (schema, table, typed filter, column filters, sort) → `SELECT`, `COUNT(*)` and `ORDER BY` per dialect; `check_where`; `ColumnFilter` ("Filter by value"). |
| `local.rs` | `local_view`: row order after local sort, column filters and text search (NULL first, numbers numerically, stable). |
| `dml.rs` | `choose_key` (primary key, else a `NOT NULL` unique key) and `preview` (the DML of pending changes, composite keys and NULLs included). |
| `nav.rs` | `referenced_row`, `referencing_rows` and `incoming`: the target table and condition of foreign-key navigation. |
| `aggregate.rs` | Count, NULLs, distinct, sum, average, min and max of cells; per column for the Aggregate view. |
| `compare.rs` | `compare`: added, removed, changed (with the changed cells) and unchanged rows by key; without a key, whole rows as a multiset. |
| `chart.rs` | `chart_data`: categories and series for bar, line, area, pie and scatter charts, grouped and aggregated. |
| `value.rs` | `editor_kind`, `format_json` (keeps key order and spellings), `format_xml`, `binary_info`, `image_mime`, `normalize_temporal`, `enum_values`. |
| `api.rs` | The requests of the IPC commands, shared by `app/src-tauri/src/commands/dataedit.rs` and `crates/forge-wasm/src/dataedit.rs`. |

## Main public items

```rust
pub use query::{check_where, ColumnFilter, FilterOp, SortKey, TableQuery, TableSql};
pub use local::{local_view, LocalView};
pub use dml::{choose_key, preview, KeyChoice, UniqueKey};
pub use nav::{referenced_row, referencing_rows, FkRef, Navigation};
pub use aggregate::{aggregate, aggregate_columns, Aggregate, ColumnAggregate};
pub use compare::{compare, DataDiff, DataSet, DiffRow, RowStatus};
pub use chart::{chart_data, ChartData, ChartSpec};
```

## Used by

- `forge-db`: `DbService::table_data` (the SQL of the filter and sort), `preview_changes` and
  `apply_changes` (row keys and DML).
- IPC: `db_table_data`, `db_preview_changes`, `db_apply_changes` (`commands/db.rs`) and the pure
  `db_data_view`, `db_data_aggregate`, `db_data_compare`, `db_data_chart`, `db_fk_navigate`,
  `db_fk_incoming`, `db_value_editor`, `db_value_format`, `db_value_binary`,
  `db_value_temporal` (`commands/dataedit.rs`).
- UI: `app/ui/src/features/database` (`TableEditor`, `ResultGrid`, `ValueEditor`, `ChartView`,
  `CompareView`, `dataTools`), client `app/ui/src/ipc/dataedit.ts`.
- Browser mode: `forge-wasm` (`dataedit_*`), mocks in `app/ui/src/mock/dataeditMock.ts` and
  `dbMock.ts`.

## Dependencies

- Workspace crates: `forge-sql`
- External: `hex`, `serde`, `serde_json`

## Tests

Unit tests for the sort/filter SQL of every SQL dialect and the refused filters, DML with
composite keys, NULLs and DEFAULT, key choice, foreign-key navigation, local sort and filter,
aggregates, data compare, charts and the value editor. End-to-end against SQLite in
`crates/forge-db/tests/sqlite.rs`.

```sh
cargo test -p forge-dataedit
cargo test -p forge-db --test sqlite
```
