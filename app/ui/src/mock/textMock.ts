/**
 * Browser-mode text files: encodings, line separators, the on-save cleanup and file formats
 * run through the same Rust code as the desktop app (`forge-text` as WebAssembly). Files
 * saved in an encoding other than UTF-8 keep their bytes here; other mocks see the text.
 */
import { registerMock } from "@/ipc";
import type { Eol, FileFormatRequest } from "@/ipc/text";
import { callWasm, forgeWasm } from "@/wasm";
import { mockFiles } from "./workspaceMock";
import { mockSettingLayers, mockSettingValues } from "./settingsMock";

/** Raw bytes of files written in another encoding than UTF-8 (or with a BOM). */
const mockBytes = new Map<string, number[]>();

const utf8 = new TextEncoder();

function bytesOf(path: string): number[] {
  const stored = mockBytes.get(path);
  if (stored) return stored;
  const content = mockFiles[path];
  if (content === undefined) throw new Error(`not found: ${path}`);
  return Array.from(utf8.encode(content));
}

registerMock("fs_read_text", async ({ path, encoding }) =>
  callWasm((await forgeWasm()).text_read, { bytes: bytesOf(String(path)), encoding: encoding ?? null }),
);

registerMock("fs_write_text", async ({ path, text, eol, encoding, bom }) => {
  const wasm = await forgeWasm();
  const { bytes } = await callWasm<{ bytes: number[] }>(wasm.text_write, { text, eol, encoding, bom });
  const p = String(path);
  const sep = (eol as Eol | undefined) ?? "\n";
  if (encoding && encoding !== "UTF-8") mockBytes.set(p, bytes);
  else if (bom) mockBytes.set(p, bytes);
  else mockBytes.delete(p);
  mockFiles[p] = sep === "\n" ? String(text) : String(text).replaceAll("\n", sep);
});

registerMock("text_prepare_save", async ({ request }) => {
  const wasm = await forgeWasm();
  return callWasm(wasm.text_prepare_save, request);
});

registerMock("vcs_inspect_commit_message", async ({ message, rules }) => {
  const wasm = await forgeWasm();
  return callWasm(wasm.text_inspect_commit_message, { message, rules });
});

/** `.editorconfig` files of the project root and every folder down to `path`. */
function editorConfigsFor(path: string): { dir: string; content: string }[] {
  const parts = path.split("/");
  const dirs = [""];
  for (let i = 1; i < parts.length; i++) dirs.push(parts.slice(0, i).join("/"));
  return dirs
    .map((dir) => ({ dir, content: mockFiles[dir ? `${dir}/.editorconfig` : ".editorconfig"] }))
    .filter((f): f is { dir: string; content: string } => f.content !== undefined);
}

// The code style comes from the selected scheme, as `editor_file_format` resolves it.
registerMock("editor_file_format", async ({ request }) => {
  const wasm = await forgeWasm();
  const r = request as FileFormatRequest;
  return callWasm(wasm.codestyle_file_format, {
    ...mockSettingLayers(),
    request: { ...r, settings: mockSettingValues(), editorconfigs: editorConfigsFor(r.path) },
  });
});
