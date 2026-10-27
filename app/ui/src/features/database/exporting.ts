/** Export a result set to a file (save dialog / browser download) or the clipboard: CSV,
 * JSON, SQL INSERTs, Markdown, and Excel workbooks (`forge-dbtransfer`). */
import { isTauri } from "@/ipc";
import { dbApi, type Cell, type Dialect, type ExportFormat } from "@/ipc/db";
import { dbObjectsApi } from "@/ipc/dbObjects";
import { showBalloon } from "@/keymap";
import type { MenuItem } from "./ContextMenu";

const LABELS: Record<ExportFormat, string> = { csv: "CSV", json: "JSON", sql: "SQL Inserts", markdown: "Markdown" };
const EXTENSIONS: Record<ExportFormat, string> = { csv: "csv", json: "json", sql: "sql", markdown: "md" };

export interface ExportSource {
  dialect: Dialect;
  table?: string;
  columns: string[];
  rows: Cell[][];
}

const plural = (n: number) => `${n} row${n === 1 ? "" : "s"}`;

export async function exportToFile(format: ExportFormat, src: ExportSource): Promise<void> {
  const ext = EXTENSIONS[format];
  const name = `${src.table ?? "result"}.${ext}`;
  try {
    if (isTauri()) {
      const { save } = await import("@tauri-apps/plugin-dialog");
      const path = await save({ defaultPath: name, filters: [{ name: LABELS[format], extensions: [ext] }] });
      if (!path) return;
      await dbApi.export({ format, ...src, path });
      showBalloon(`Exported ${plural(src.rows.length)}`, path);
      return;
    }
    const text = await dbApi.export({ format, ...src });
    const url = URL.createObjectURL(new Blob([text], { type: "text/plain" }));
    const a = document.createElement("a");
    a.href = url;
    a.download = name;
    a.click();
    URL.revokeObjectURL(url);
    showBalloon(`Exported ${plural(src.rows.length)}`, name);
  } catch (e) {
    showBalloon("Export failed", e instanceof Error ? e.message : String(e));
  }
}

export async function copyAs(format: ExportFormat, src: ExportSource): Promise<void> {
  try {
    const text = await dbApi.export({ format, ...src });
    await navigator.clipboard?.writeText(text);
    showBalloon(`Copied ${plural(src.rows.length)} as ${LABELS[format]}`);
  } catch (e) {
    showBalloon("Copy failed", e instanceof Error ? e.message : String(e));
  }
}

/** An Excel workbook of the rows (save dialog / browser download). */
export async function exportToExcel(src: ExportSource): Promise<void> {
  const name = `${src.table ?? "result"}.xlsx`;
  const sheet = src.table ?? "Result";
  try {
    if (isTauri()) {
      const { save } = await import("@tauri-apps/plugin-dialog");
      const path = await save({ defaultPath: name, filters: [{ name: "Excel", extensions: ["xlsx"] }] });
      if (!path) return;
      await dbObjectsApi.exportXlsx(sheet, src.columns, src.rows, path);
      showBalloon(`Exported ${plural(src.rows.length)}`, path);
      return;
    }
    const b64 = (await dbObjectsApi.exportXlsx(sheet, src.columns, src.rows)) ?? "";
    const bytes = Uint8Array.from(atob(b64), (c) => c.charCodeAt(0));
    const url = URL.createObjectURL(new Blob([bytes], { type: "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet" }));
    const a = document.createElement("a");
    a.href = url;
    a.download = name;
    a.click();
    URL.revokeObjectURL(url);
    showBalloon(`Exported ${plural(src.rows.length)}`, name);
  } catch (e) {
    showBalloon("Export failed", e instanceof Error ? e.message : String(e));
  }
}

/** Menu entries of the "Export Data" button. */
export function exportMenu(src: () => ExportSource): MenuItem[] {
  const formats: ExportFormat[] = ["csv", "json", "sql", "markdown"];
  return [
    ...formats.map((f) => ({ label: `Export to ${LABELS[f]} File…`, run: () => void exportToFile(f, src()) })),
    { label: "Export to Excel File…", run: () => void exportToExcel(src()) },
    { separator: true, label: "" },
    ...formats.map((f) => ({ label: `Copy as ${LABELS[f]}`, run: () => void copyAs(f, src()) })),
  ];
}
