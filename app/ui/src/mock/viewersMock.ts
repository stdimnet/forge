/**
 * Browser-mode image editor and "Edit as Table": the Rust `forge-viewers` crate via
 * WebAssembly, like the desktop app. Images come from the demo project (PNG bytes in
 * `demoDocs.ts`, SVG as text); nothing is written anywhere.
 */
import { registerMock } from "@/ipc";
import type { CsvTable, ImageData } from "@/ipc/viewers";
import { callWasm, forgeWasm } from "@/wasm";
import { demoImages } from "./demoDocs";
import { mockFiles } from "./workspaceMock";

function utf8Base64(text: string): string {
  const bytes = new TextEncoder().encode(text);
  let bin = "";
  for (const b of bytes) bin += String.fromCharCode(b);
  return btoa(bin);
}

registerMock("image_read", async ({ path }) => {
  const p = String(path);
  const base64 = demoImages[p] ?? (mockFiles[p] !== undefined ? utf8Base64(mockFiles[p]) : undefined);
  if (base64 === undefined) throw new Error(`not found: ${p}`);
  return callWasm<ImageData>((await forgeWasm()).image_describe, { path: p, base64 });
});

registerMock("csv_parse", async ({ text, path, delimiter }) =>
  callWasm<CsvTable>((await forgeWasm()).csv_parse, { text: String(text ?? ""), path: path ?? null, delimiter: delimiter ?? null }),
);

registerMock("csv_edit", async ({ text, delimiter, edits }) =>
  callWasm<string>((await forgeWasm()).csv_edit, { text: String(text ?? ""), delimiter, edits }),
);

registerMock("csv_sort_order", async ({ rows, column, descending, headerRows }) =>
  callWasm<number[]>((await forgeWasm()).csv_sort_order, { rows, column, descending, headerRows }),
);
