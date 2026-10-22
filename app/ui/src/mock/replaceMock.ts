/** Browser-mode Replace in Files: the demo project lives in memory, so a replace and its undo do too. */
import { registerMock } from "@/ipc";
import type { FindRequest, ReplaceDone, ReplacePreviewFile, UndoneReplace } from "@/ipc/find";
import { callWasm, forgeWasm } from "@/wasm";
import { mockFiles } from "./workspaceMock";

interface Replaced {
  files: { path: string; text: string; count: number }[];
}

let last: { path: string; before: string; after: string }[] = [];

/** Keep files a replace changed for `undo_replace` (Structural Replace's mock uses it too). */
export function rememberReplace(changed: { path: string; before: string; after: string }[]): void {
  last = changed;
}

const filesOf = (): [string, string][] => Object.keys(mockFiles).sort().map((p) => [p, mockFiles[p]]);

registerMock("replace_preview", async ({ request, with: withText }) => {
  const w = await forgeWasm();
  return callWasm<ReplacePreviewFile[]>(w.replace_preview, { request: request as FindRequest, with: withText, files: filesOf() });
});

registerMock("replace_in_files", async ({ request, with: withText }): Promise<ReplaceDone> => {
  const w = await forgeWasm();
  const out = await callWasm<Replaced>(w.replace_in_files, { request: request as FindRequest, with: withText, files: filesOf() });
  last = out.files.map((f) => ({ path: f.path, before: mockFiles[f.path], after: f.text }));
  for (const f of out.files) mockFiles[f.path] = f.text;
  return { files: out.files.length, replacements: out.files.reduce((n, f) => n + f.count, 0), skipped: [] };
});

registerMock("undo_replace", (): UndoneReplace => {
  const left: string[] = [];
  let restored = 0;
  for (const r of last) {
    if (mockFiles[r.path] === r.after) {
      mockFiles[r.path] = r.before;
      restored++;
    } else left.push(r.path);
  }
  last = [];
  return { restored, left };
});
