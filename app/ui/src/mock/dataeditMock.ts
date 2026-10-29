/**
 * Browser-mode data editor commands: the same Rust (`forge-dataedit`) as the desktop app,
 * through WebAssembly. The table-backed commands (`db_table_data`, `db_preview_changes`,
 * `db_apply_changes`) are in `dbMock.ts`.
 */
import { registerMock } from "@/ipc";
import { callWasm, forgeWasm, type ForgeWasm } from "@/wasm";

const pure = (cmd: string, pick: (w: ForgeWasm) => (json: string) => string) =>
  registerMock(cmd, async (a) => {
    const wasm = await forgeWasm();
    return callWasm(pick(wasm), a.request);
  });

pure("db_data_view", (w) => w.dataedit_view);
pure("db_data_aggregate", (w) => w.dataedit_aggregate);
pure("db_data_compare", (w) => w.dataedit_compare);
pure("db_data_chart", (w) => w.dataedit_chart);
pure("db_fk_navigate", (w) => w.dataedit_navigate);
pure("db_fk_incoming", (w) => w.dataedit_incoming);
pure("db_value_editor", (w) => w.dataedit_value_editor);
pure("db_value_format", (w) => w.dataedit_value_format);
pure("db_value_binary", (w) => w.dataedit_value_binary);
pure("db_value_temporal", (w) => w.dataedit_value_temporal);
