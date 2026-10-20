/** Export a result set to a file (save dialog / browser download) or the clipboard. */
import { isTauri } from "@/ipc";
import { dbApi, type Cell, type Dialect, type ExportFormat } from "@/ipc/db";
import { showBalloon } from "@/keymap";
import type { MenuItem } from "./ContextMenu";

const LABELS: Record<ExportFormat, string> = { csv: "CSV", json: "JSON", sql: "SQL Inserts" };

export interface ExportSource {
  dialect: Dialect;
  table?: string;
  columns: string[];
  rows: Cell[][];
}

const plural = (n: number) => `${n} row${n === 1 ? "" : "s"}`;

export async function exportToFile(format: ExportFormat, src: ExportSource): Promise<void> {
  const ext = format === "sql" ? "sql" : format;
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

/** Menu entries of the "Export Data" button. */
export function exportMenu(src: () => ExportSource): MenuItem[] {
  const formats: ExportFormat[] = ["csv", "json", "sql"];
  return [
    ...formats.map((f) => ({ label: `Export to ${LABELS[f]} File…`, run: () => void exportToFile(f, src()) })),
    { separator: true, label: "" },
    ...formats.map((f) => ({ label: `Copy as ${LABELS[f]}`, run: () => void copyAs(f, src()) })),
  ];
}
