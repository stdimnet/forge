/**
 * Browser-mode Extract Component and Refactor This for JS/TS/Vue files. The analysis runs the
 * same Rust code as the desktop app (`webforge_fw_vue::extract`, `webforge_fw_react::extract`
 * as WebAssembly); only the in-memory demo files change. There is no TypeScript language
 * server in the browser, so Refactor This offers a few typical server refactorings for a
 * selection, worded by the Rust mapping (`forge_codeintel::refactor_this_actions`).
 */
import { registerMock } from "@/ipc";
import type { ContextAction } from "@/ipc/codeintel";
import type { LspPosition } from "@/ipc/lsp";
import type { ExtractComponentInfo, RefactorAnswer, RefactorThisAnswer } from "@/ipc/refactor";
import { callWasm, forgeWasm } from "@/wasm";
import { offsetOf, positionOf } from "./codeintelMock";
import { Plan } from "./moveMock";
import { mockFiles } from "./workspaceMock";

const encoder = new TextEncoder();
/** JS string index -> UTF-8 byte offset (what the Rust code works with). */
const byteOf = (text: string, index: number) => encoder.encode(text.slice(0, index)).length;
/** UTF-8 byte offset -> JS string index. */
function indexOf(text: string, byte: number): number {
  let bytes = 0;
  for (let i = 0; i < text.length; i++) {
    if (bytes >= byte) return i;
    const c = text.codePointAt(i)!;
    bytes += c < 0x80 ? 1 : c < 0x800 ? 2 : c < 0x10000 ? 3 : 4;
    if (c >= 0x10000) i++;
  }
  return text.length;
}

export const isWebScript = (path: string) => /\.(vue|jsx|tsx|js|ts|mjs|cjs)$/i.test(path) && !/\.d\.ts$/i.test(path);
const isVueOrJsx = (path: string) => /\.(vue|jsx|tsx|js|ts|mjs)$/i.test(path) && !/\.d\.ts$/i.test(path);

interface Range {
  start: LspPosition;
  end: LspPosition;
}

function bytesOf(text: string, range: Range): [number, number] {
  const a = byteOf(text, offsetOf(text, range.start));
  const b = byteOf(text, offsetOf(text, range.end));
  return [Math.min(a, b), Math.max(a, b)];
}

/** Extract Component's analysis, or `null` when it does not apply. */
export async function extractComponentInfo(path: string, text: string, range: Range): Promise<ExtractComponentInfo | null> {
  if (!isVueOrJsx(path)) return null;
  const [start, end] = bytesOf(text, range);
  if (start === end) return null;
  const wasm = await forgeWasm();
  return callWasm<ExtractComponentInfo>(wasm.web_extract_component_prepare, { path, content: text, start, end }).catch(() => null);
}

registerMock("refactor_extract_component_prepare", async ({ path, start, end, content }): Promise<ExtractComponentInfo> => {
  const text = String(content);
  const [s, e] = bytesOf(text, { start: start as LspPosition, end: end as LspPosition });
  const wasm = await forgeWasm();
  return callWasm<ExtractComponentInfo>(wasm.web_extract_component_prepare, { path, content: text, start: s, end: e });
});

registerMock("refactor_extract_component", async (args): Promise<RefactorAnswer> => {
  const { path, start, end, content, name, directory } = args as {
    path: string;
    start: LspPosition;
    end: LspPosition;
    content: string;
    name: string;
    directory: string;
  };
  const [s, e] = bytesOf(content, { start, end });
  const wasm = await forgeWasm();
  const info = await callWasm<ExtractComponentInfo>(wasm.web_extract_component_prepare, { path, content, start: s, end: e });
  const dir = directory.trim().replace(/^\/+|\/+$/g, "");
  const newPath = `${dir ? `${dir}/` : ""}${name.trim()}.${info.extension}`;
  if (mockFiles[newPath] !== undefined) throw new Error(`File ${newPath} already exists`);
  const out = await callWasm<{ path: string; text: string; edits: { start: number; end: number; text: string }[] }>(wasm.web_extract_component, {
    path,
    content,
    start: s,
    end: e,
    name: name.trim(),
    newPath,
  });
  const plan = new Plan({ [path]: content });
  plan.edit.changes.push({ kind: "create", path: out.path, overwrite: false });
  plan.edit.changes.push({ kind: "edit", path: out.path, edits: [{ range: { start: { line: 0, character: 0 }, end: { line: 0, character: 0 } }, newText: out.text }] });
  plan.edit.changes.push({
    kind: "edit",
    path,
    edits: out.edits.map((x) => ({
      range: { start: positionOf(content, indexOf(content, x.start)), end: positionOf(content, indexOf(content, x.end)) },
      newText: x.text,
    })),
  });
  return plan.answer();
});

/** The typical TypeScript server refactorings for a selection (no server in the browser). */
const SERVER_REFACTORINGS: [string, string][] = [
  ["Extract to function in module scope", "refactor.extract.function"],
  ["Extract to constant in enclosing scope", "refactor.extract.constant"],
  ["Move to a new file", "refactor.move.newFile"],
];

/** Refactor This in JS/TS/Vue files. */
export async function webRefactorThis(path: string, text: string, range: Range): Promise<RefactorThisAnswer> {
  const component = await extractComponentInfo(path, text, range);
  const actions = ["RenameElement", ...(component ? ["WebForge.ExtractComponent"] : []), "Move", "CopyElement"];
  const [s, e] = bytesOf(text, range);
  const raw: ContextAction[] =
    s === e || component
      ? []
      : SERVER_REFACTORINGS.map(([title, kind]) => ({
          title,
          kind: "refactor",
          source: "lsp",
          server: "typescript-language-server",
          isPreferred: false,
          needsResolve: false,
          raw: { title, kind },
        }));
  const wasm = await forgeWasm();
  const lsp = raw.length ? await callWasm<ContextAction[]>(wasm.codeintel_refactor_this_actions, raw) : [];
  return { actions, lsp };
}
