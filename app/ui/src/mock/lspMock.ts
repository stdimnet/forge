/**
 * Browser-mode language server: behaves like Intelephense on the demo project (hover
 * docs, completion, diagnostics, go to declaration, parameter info, formatting, status).
 */
import { emitMock, registerMock } from "@/ipc";
import type {
  EditorCompletionItem,
  EditorCompletionList,
  EditorDiagnostic,
  EditorLocation,
  EditorSignatureHelp,
  EditorSymbol,
  LspPosition,
  LspRange,
  ServerStatus,
  TextEdit,
} from "@/ipc/lsp";
import { demoFiles } from "./demoProject";
import {
  collectionMembers,
  diagnosticRules,
  globals,
  hoverDocs,
  modelMembers,
  orderMembers,
  signatures,
  staticMembers,
  variables,
} from "./lspDemoData";

const docs: Record<string, string> = { ...demoFiles };

// ---------- text helpers ----------

export function offsetAt(text: string, pos: LspPosition): number {
  let offset = 0;
  const lines = text.split("\n");
  for (let i = 0; i < pos.line && i < lines.length; i++) offset += lines[i].length + 1;
  return offset + Math.min(pos.character, lines[pos.line]?.length ?? 0);
}

export function positionAt(text: string, offset: number): LspPosition {
  const before = text.slice(0, offset).split("\n");
  return { line: before.length - 1, character: before[before.length - 1].length };
}

const rangeOf = (text: string, from: number, to: number): LspRange => ({
  start: positionAt(text, from),
  end: positionAt(text, to),
});

const isWordChar = (c: string | undefined) => !!c && /[\w$]/.test(c);

/** The identifier (incl. a leading `$`) at `offset`, with its bounds. */
export function wordAt(text: string, offset: number): { word: string; from: number; to: number } | null {
  let from = offset;
  let to = offset;
  while (isWordChar(text[from - 1])) from--;
  while (isWordChar(text[to])) to++;
  if (from === to) return null;
  return { word: text.slice(from, to), from, to };
}

// ---------- diagnostics ----------

export function diagnosticsFor(path: string, text: string): EditorDiagnostic[] {
  return diagnosticRules
    .filter((r) => r.file === path)
    .flatMap((r) => {
      const at = text.indexOf(r.find);
      if (at < 0) return [];
      const inner = r.part ? r.find.indexOf(r.part) : 0;
      const from = at + Math.max(0, inner);
      const to = from + (r.part ?? r.find).length;
      return [
        {
          range: rangeOf(text, from, to),
          severity: r.severity,
          message: r.message,
          source: "intelephense",
          code: null,
          unnecessary: !!r.unnecessary,
          deprecated: !!r.deprecated,
        },
      ];
    });
}

function publish(path: string) {
  const text = docs[path];
  if (text === undefined || !path.endsWith(".php")) return;
  emitMock("lsp://diagnostics", { path, diagnostics: diagnosticsFor(path, text) });
}

// ---------- server status ----------

let status: ServerStatus = { id: "intelephense", name: "Intelephense", state: "notStarted" };
let started = false;
/** Other mock servers (e.g. the TypeScript server of the language servers mock). */
const otherServers = new Map<string, ServerStatus>();
const allStatuses = () => [status, ...otherServers.values()];
const setStatus = (s: ServerStatus) => {
  status = s;
  emitMock("lsp://status", allStatuses());
};

/** Publish the state of another mock server next to Intelephense. */
export function setMockServerStatus(s: ServerStatus): void {
  otherServers.set(s.id, s);
  emitMock("lsp://status", allStatuses());
}

const openHooks: ((path: string) => void)[] = [];
/** Called for every `lsp_did_open`, so other mocks can start their servers lazily. */
export function onMockDidOpen(cb: (path: string) => void): void {
  openHooks.push(cb);
}

function startServer() {
  if (started) return;
  started = true;
  const base = { id: "intelephense", name: "Intelephense" };
  setStatus({ ...base, state: "starting" });
  const steps = [0, 35, 70, 100];
  steps.forEach((p, i) =>
    setTimeout(() => setStatus({ ...base, state: "busy", message: "Indexing", percentage: p }), 120 + i * 150),
  );
  setTimeout(
    () => {
      setStatus({ ...base, state: "ready" });
      // Like Intelephense after indexing: diagnostics for the whole workspace.
      Object.keys(docs).forEach(publish);
    },
    120 + steps.length * 150,
  );
}

// ---------- hover ----------

export function hoverAt(path: string, pos: LspPosition): { contents: string; range: LspRange } | null {
  const text = docs[path];
  if (text === undefined) return null;
  const w = wordAt(text, offsetAt(text, pos));
  if (!w) return null;
  const name = w.word.replace(/^\$/, "");
  const isCall = text[w.to] === "(";
  const key = isCall ? `${name}()` : name;
  const contents = hoverDocs[key] ?? hoverDocs[isCall ? name : `${name}()`];
  return contents ? { contents, range: rangeOf(text, w.from, w.to) } : null;
}

// ---------- completion ----------

export function completionAt(path: string, pos: LspPosition): EditorCompletionList {
  const text = docs[path] ?? "";
  const offset = offsetAt(text, pos);
  const line = text.slice(text.lastIndexOf("\n", offset - 1) + 1, offset);
  const prefixMatch = /[\w$]*$/.exec(line)!;
  const before = line.slice(0, line.length - prefixMatch[0].length);
  let items;
  if (/(orders|collect\([^)]*\))->$/.test(before)) items = collectionMembers;
  else if (/\$order->$/.test(before)) items = orderMembers;
  else if (/->$/.test(before)) items = modelMembers;
  else if (/::$/.test(before)) items = staticMembers;
  else if (prefixMatch[0].startsWith("$")) items = variables;
  else items = globals;
  const toItem = (i: Omit<EditorCompletionItem, "raw">, idx: number): EditorCompletionItem => {
    const key = i.kind === "method" || i.kind === "function" ? `${i.label}()` : i.label;
    return {
      ...i,
      documentation: hoverDocs[key] ?? null,
      sortText: String(idx).padStart(3, "0"),
      raw: { label: i.label },
    };
  };
  return { isIncomplete: false, items: items.map(toItem) };
}

// ---------- signature help ----------

export function signatureAt(path: string, pos: LspPosition): EditorSignatureHelp | null {
  const text = docs[path] ?? "";
  const offset = offsetAt(text, pos);
  let depth = 0;
  let commas = 0;
  for (let i = offset - 1; i >= 0 && i > offset - 2000; i--) {
    const c = text[i];
    if (c === ")" || c === "]") depth++;
    else if (c === "[") depth--;
    else if (c === "(") {
      if (depth === 0) {
        const w = wordAt(text, i);
        const def = w ? signatures[w.word] : undefined;
        if (!def) return null;
        let from = 0;
        const parameters = def.params.map((p) => {
          const start = def.label.indexOf(p, from);
          from = start + p.length;
          return { label: [start, start + p.length] as [number, number] };
        });
        return {
          signatures: [{ label: def.label, parameters, documentation: null }],
          activeSignature: 0,
          activeParameter: Math.min(commas, Math.max(0, parameters.length - 1)),
        };
      }
      depth--;
    } else if (c === "," && depth === 0) commas++;
    else if (c === ";" || c === "{" || c === "}") return null;
  }
  return null;
}

// ---------- navigation ----------

const DECL = (name: string) =>
  new RegExp(`(?:class|enum|trait|interface|function)\\s+(${name.replace(/[$]/g, "")})\\b|case\\s+(${name})\\b`);

export function definitionAt(path: string, pos: LspPosition): EditorLocation[] {
  const text = docs[path] ?? "";
  const w = wordAt(text, offsetAt(text, pos));
  if (!w) return [];
  const name = w.word.replace(/^\$/, "");
  // Imported aliases (`as Authenticatable`) and vendor classes are not part of the demo.
  for (const [file, content] of Object.entries(docs)) {
    const m = DECL(name).exec(content);
    if (!m) continue;
    const start = m.index + m[0].lastIndexOf(name);
    return [{ path: file, range: rangeOf(content, start, start + name.length) }];
  }
  return [];
}

export function referencesAt(path: string, pos: LspPosition): EditorLocation[] {
  const text = docs[path] ?? "";
  const w = wordAt(text, offsetAt(text, pos));
  if (!w) return [];
  const name = w.word.replace(/^\$/, "");
  const re = new RegExp(`\\b${name}\\b`, "g");
  return Object.entries(docs).flatMap(([file, content]) =>
    [...content.matchAll(re)].map((m) => ({ path: file, range: rangeOf(content, m.index!, m.index! + name.length) })),
  );
}

export function symbolsOf(path: string): EditorSymbol[] {
  const text = docs[path] ?? "";
  const sym = (name: string, kind: string, at: number): EditorSymbol => {
    const range = rangeOf(text, at, at + name.length);
    return { name, kind, detail: null, range, selectionRange: range, children: [] };
  };
  const roots: EditorSymbol[] = [];
  for (const m of text.matchAll(/(class|enum|trait|interface)\s+(\w+)/g)) {
    roots.push(sym(m[2], m[1] === "enum" ? "enum" : m[1], m.index! + m[0].length - m[2].length));
  }
  const parent = roots[0];
  for (const m of text.matchAll(/function\s+(\w+)/g)) {
    const s = sym(m[1], m[1] === "__construct" ? "constructor" : "method", m.index! + m[0].length - m[1].length);
    (parent ? parent.children : roots).push(s);
  }
  for (const m of text.matchAll(/(?:private|protected|public)\s+(?:readonly\s+)?(?:[\w|?\\]+\s+)?(\$\w+)\s*[;=]/g)) {
    (parent ? parent.children : roots).push(sym(m[1], "property", m.index! + m[0].indexOf(m[1])));
  }
  return roots;
}

// ---------- formatting ----------

export function formatEdits(text: string): TextEdit[] {
  const formatted =
    text
      .split("\n")
      .map((l) => l.replace(/\t/g, "    ").replace(/\s+$/, ""))
      .join("\n")
      .replace(/\n{3,}/g, "\n\n")
      .replace(/\s*$/, "") + "\n";
  if (formatted === text) return [];
  return [{ range: rangeOf(text, 0, text.length), newText: formatted }];
}

// ---------- handlers ----------

const str = (v: unknown) => String(v ?? "");
const posOf = (a: Record<string, unknown>): LspPosition => ({ line: Number(a.line), character: Number(a.character) });
const changeTimers = new Map<string, ReturnType<typeof setTimeout>>();

registerMock("lsp_did_open", ({ path, text }) => {
  docs[str(path)] = str(text);
  startServer();
  if (status.state === "ready") setTimeout(() => publish(str(path)), 30);
  openHooks.forEach((h) => h(str(path)));
});
registerMock("lsp_did_change", ({ path, text }) => {
  const p = str(path);
  docs[p] = str(text);
  clearTimeout(changeTimers.get(p));
  changeTimers.set(
    p,
    setTimeout(() => publish(p), 250),
  );
});
registerMock("lsp_did_save", () => undefined);
registerMock("lsp_did_close", () => undefined);
registerMock("lsp_hover", (a) => hoverAt(str(a.path), posOf(a)));
registerMock("lsp_completion", (a) => completionAt(str(a.path), posOf(a)));
registerMock("lsp_completion_resolve", () => null);
registerMock("lsp_definition", (a) => definitionAt(str(a.path), posOf(a)));
registerMock("lsp_references", (a) => referencesAt(str(a.path), posOf(a)));
registerMock("lsp_document_symbols", (a) => symbolsOf(str(a.path)));
registerMock("lsp_signature_help", (a) => signatureAt(str(a.path), posOf(a)));
registerMock("lsp_format", (a) => formatEdits(docs[str(a.path)] ?? ""));
registerMock("lsp_status", () => allStatuses());
registerMock("lsp_register_server", () => undefined);
