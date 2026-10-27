---
title: forge-dbtransfer
description: "Forge database import, export and dump (pure, also built to WebAssembly): CSV/TSV, Excel and JSON sources with type detection, batched INSERTs per dialect, Excel export and the command lines of mysqldump, pg_dump, pg_restore, psql, sqlite3 and sqlcmd."
order: 62
tags: [crate, rust]
---

# forge-dbtransfer

`crates/forge-dbtransfer` — moving data in and out of databases: the pure part of the import
dialog, Export to Excel and the native dump / restore tools. Built into `forge-db` (desktop) and
into the browser-mode backend (WebAssembly).

## Purpose

- `read`: import sources. CSV / TSV per RFC 4180 with the usual quirks (byte order mark, CRLF,
  quoted line breaks, doubled quotes, a quote inside an unquoted field, ragged rows padded and
  reported, blank lines skipped, an unclosed quote reported with its line), a sniffed delimiter
  (`,` `;` tab `|`: the one that splits the first lines into the same number of fields), a custom
  quote, header row, skipped rows and trimming; encodings through `encoding_rs` (any WHATWG
  label; detected from a BOM, else UTF-8, else Windows-1252). Excel workbooks (`.xlsx`, `.xls`,
  `.xlsb`, `.ods`) through `calamine`: sheets, numbers (integral floats without `.0`), booleans,
  dates and date-times, errors as empty cells. JSON: an array of objects (columns are every key
  in first-seen order; nested values as JSON text), an array of arrays (a string-only first row
  is the header) or NDJSON. Empty CSV fields and JSON `null` are `None`, `""` is `Some("")`.
- `detect`: column kinds (integer, decimal with precision and scale, boolean, date, date-time,
  text; leading zeros and 19+ digit numbers stay text), the SQL type of a new column per
  dialect, target column names (`Order Date` → `order_date`) and the new table as a
  `forge_dbmodel::design::TableSpec`.
- `insert`: the import plan (target, column mapping with kinds, optional new table, batch size,
  skip or abort, empty values as NULL, row limit), literals per kind and dialect (Oracle
  `DATE '…'` / `TIMESTAMP '…'`, booleans as `TRUE`/`FALSE` or `1`/`0`, numbers unquoted only
  when they are numbers) and the batched `INSERT` statements (at most 1000 rows per statement on
  SQL Server, `INSERT ALL … SELECT 1 FROM dual` on Oracle) plus the single-row insert of a skip
  retry.
- `xlsx`: a single-sheet `.xlsx` workbook (stored ZIP entries written by hand, inline strings,
  numbers and booleans as such, a bold frozen header row), small enough for the browser build.
- `dump`: the tools of a driver and the command line of a dump or restore for a data source's
  connection — `mysqldump` / `mysql`, `pg_dump` / `pg_restore` / `psql`, `sqlite3` dot-commands
  and `sqlcmd` (`BACKUP DATABASE`, `RESTORE DATABASE`, `-i script.sql`), with schemas, tables
  and options, and extra arguments split like a shell. Passwords go into `MYSQL_PWD`,
  `PGPASSWORD` or `SQLCMDPASSWORD`, never into the arguments.

### Decisions

- **calamine** (MIT) reads Excel files; it is pure Rust and builds to WebAssembly, so browser
  mode previews workbooks too. It sits behind the default feature `xlsx`.
- The Excel writer is hand-written (about 200 lines) instead of a writer crate: export needs one
  sheet of values, and stored ZIP entries need no compression code in the WebAssembly build.
- Running the tools, opening SSH tunnels and executing inserts is `forge-db`'s job; this crate
  only computes text and command lines, so all of it is unit-tested without a database.

## Modules

| Module | Summary |
| --- | --- |
| `read.rs` | `SourceFormat`, `ReadOptions`, `SourceTable`, `read`, `decode`, `sniff_delimiter`, `parse_csv`, `read_json`. |
| `detect.rs` | `ValueKind`, `ColumnGuess`, `classify`, `detect`, `sql_type`, `column_ident`, `new_table`. |
| `insert.rs` | `ImportPlan`, `ColumnMapping`, `OnError`, `Batch`, `literal`, `insert_sql`, `batches`, `row_insert`. |
| `xlsx.rs` | `workbook(sheet, columns, rows)`. |
| `dump.rs` | `DumpTool`, `DumpAction`, `DumpTarget`, `DumpRequest`, `ToolCommand`, `tools_for`, `command`, `split_args`. |

## Main public items

```rust
pub use detect::{detect, ColumnGuess, ValueKind};
pub use dump::{DumpAction, DumpRequest, DumpTarget, DumpTool, ToolCommand};
pub use insert::{Batch, ColumnMapping, ImportPlan, OnError};
pub use read::{read, ReadOptions, SourceFormat, SourceTable};
pub fn preview(bytes: &[u8], opts: &ReadOptions, rows: usize, dialect: Option<Dialect>) -> Result<Preview, String>;
```

## Used by

- [`forge-db`](forge-db.md): `DbService::import`, `DbService::dump_target`.
- IPC: `db_import_preview`, `db_import_run`, `db_import_cancel`, `db_export_xlsx`,
  `db_dump_tools`, `db_dump_command`, `db_restore_guard`, `db_dump_start`
  (`app/src-tauri/src/commands/db_objects.rs`)
- Browser mode: [`forge-wasm`](forge-wasm.md) `dbobjects_import_preview`,
  `dbobjects_import_batches`, `dbobjects_export_xlsx`, `dbobjects_dump_tools`,
  `dbobjects_dump_command` (`dbobjects.rs`)

## Dependencies

- Workspace crates: [`forge-dbmodel`](forge-dbmodel.md), [`forge-sql`](forge-sql.md)
- External: `calamine` and `chrono` (feature `xlsx`), `encoding_rs`, `serde`, `serde_json` (all
  MIT or Apache-2.0)

## Tests

6 source files; 22 unit tests (CSV quirks, delimiters, encodings, JSON and NDJSON, type
detection, SQL types, literals and batches per dialect, the Excel writer read back with
calamine, every dump command line with the password kept out of the arguments) and 2
integration tests in `tests/xlsx.rs` against a workbook written by openpyxl
(`tests/fixtures/customers.xlsx`: two sheets, dates, date-times, booleans, empty cells).

```sh
cargo test -p forge-dbtransfer
```
