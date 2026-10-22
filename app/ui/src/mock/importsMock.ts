/**
 * Browser-mode Optimize Imports and Auto Import. The tree-sitter based PHP code is not in
 * the WebAssembly build, so unresolved class names and `use` lines are found with simple
 * patterns over the demo files; the notifications, the editor change and the folder
 * targets come from `forge-codeintel` as WebAssembly. Nothing is written to a real project:
 * folder results are applied to the in-memory demo files.
 */
import { invoke, registerMock } from "@/ipc";
import type { FolderOptimizeAnswer, OffsetEdit, OnTheFlyAnswer, OnTheFlyRequest, OptimizeAnswer, UnresolvedClass } from "@/ipc/imports";
import type { ContextAction, TextEditLsp, WorkspaceEdit } from "@/ipc/codeintel";
import { callWasm, forgeWasm } from "@/wasm";
import { mockOptimizeImports } from "./saveActionsMock";
import { mockSettingValues } from "./settingsMock";
import { mockFiles } from "./workspaceMock";
import { parsePhp } from "./phpMockParser";

/** Classes of the demo's vendor folder and PHP's own classes. */
const VENDOR_CLASSES = [
  "Illuminate\\Support\\Collection",
  "Illuminate\\Database\\Eloquent\\Collection",
  "Illuminate\\Support\\Str",
  "Illuminate\\Support\\Arr",
  "Illuminate\\Support\\Facades\\DB",
  "Illuminate\\Support\\Facades\\Log",
  "Illuminate\\Support\\Carbon",
  "Carbon\\Carbon",
  "Exception",
  "InvalidArgumentException",
  "RuntimeException",
  "DateTimeImmutable",
];

const isPhp = (p: string) => /\.(php|phtml|inc)$/i.test(p) && !/\.blade\.php$/i.test(p);

function knownClasses(): string[] {
  const out = new Set(VENDOR_CLASSES);
  for (const [path, text] of Object.entries(mockFiles)) {
    if (!isPhp(path)) continue;
    const file = parsePhp(text);
    for (const c of file.classes) out.add(file.namespace ? `${file.namespace}\\${c.name}` : c.name);
  }
  return [...out];
}

const short = (fqn: string) => fqn.split("\\").pop()!;
const namespaceOf = (fqn: string) => (fqn.includes("\\") ? fqn.slice(0, fqn.lastIndexOf("\\")) : "");

const PATTERNS = [
  /\bnew\s+([A-Z]\w*)/g,
  /\b([A-Z]\w*)::/g,
  /(?:[(,]\s*|\b(?:public|private|protected|readonly)\s+)\??([A-Z]\w*)\s+\$/g,
  /\)\s*:\s*\??([A-Z]\w*)/g,
  /\b(?:extends|implements|instanceof)\s+([A-Z]\w*)/g,
  /\bcatch\s*\(\s*([A-Z]\w*)/g,
  /@(?:param|var|return|throws)\s+\??([A-Z]\w*)/g,
];
const SPECIAL = new Set(["self", "static", "parent"]);

/** Unresolved class names of a PHP text with candidates from the demo files and "vendor". */
export function mockUnresolved(text: string): UnresolvedClass[] {
  const file = parsePhp(text);
  const imported = new Set([...file.uses.keys()].map((k) => k.toLowerCase()));
  const declared = new Set(file.classes.map((c) => c.name.toLowerCase()));
  const known = knownClasses();
  const out = new Map<number, UnresolvedClass>();
  for (const re of PATTERNS) {
    for (const m of text.matchAll(re)) {
      const name = m[1];
      const start = m.index! + m[0].lastIndexOf(name);
      const lower = name.toLowerCase();
      if (SPECIAL.has(lower) || imported.has(lower) || declared.has(lower) || out.has(start)) continue;
      const resolved = file.namespace ? `${file.namespace}\\${name}` : name;
      if (known.some((k) => k.toLowerCase() === resolved.toLowerCase())) continue;
      const candidates = known.filter((k) => short(k).toLowerCase() === lower).sort((a, b) => a.localeCompare(b));
      const doc = re.source.startsWith("@");
      out.set(start, { start, end: start + name.length, name, resolved, doc, candidates });
    }
  }
  return [...out.values()].sort((a, b) => a.start - b.start);
}

/** `use <fqn>;` in alphabetical order among the `use` lines, else after the namespace. */
function importEdit(text: string, fqn: string): OffsetEdit | null {
  const file = parsePhp(text);
  if ([...file.uses.values()].some((u) => u.toLowerCase() === fqn.toLowerCase())) return null;
  const line = `use ${fqn};`;
  const uses = [...text.matchAll(/^use [^;]+;$/gm)];
  const next = uses.find((u) => u[0].toLowerCase() > line.toLowerCase());
  if (next) return { start: next.index!, end: next.index!, newText: `${line}\n` };
  const last = uses[uses.length - 1];
  if (last) {
    const at = last.index! + last[0].length;
    return { start: at, end: at, newText: `\n${line}` };
  }
  const ns = /^namespace [^;]+;/m.exec(text);
  const at = ns ? ns.index + ns[0].length : text.indexOf("\n");
  return { start: at, end: at, newText: `\n\n${line}` };
}

function referenceEdits(text: string, start: number, end: number, fqn: string): OffsetEdit[] {
  const settings = mockSettingValues();
  const clean = fqn.replace(/^\\+/, "");
  const ns = parsePhp(text).namespace;
  if (namespaceOf(clean).toLowerCase() === ns.toLowerCase()) return [];
  if (settings["php.codeStyle.imports.preferFqn"] === true) return [{ start, end, newText: `\\${clean}` }];
  const edits: OffsetEdit[] = [];
  if (text.slice(start, end) !== short(clean)) edits.push({ start, end, newText: short(clean) });
  const imp = importEdit(text, clean);
  if (imp) edits.push(imp);
  return edits;
}

function hintAt(text: string, offset: number): UnresolvedClass | null {
  const list = mockUnresolved(text).filter((u) => u.candidates.length);
  const under = list.find((u) => u.start <= offset && offset <= u.end);
  if (under) return under;
  const lineStart = text.lastIndexOf("\n", offset - 1) + 1;
  const lineEnd = text.indexOf("\n", offset) < 0 ? text.length : text.indexOf("\n", offset);
  return list.filter((u) => u.start >= lineStart && u.end <= lineEnd).sort((a, b) => Math.abs(a.start - offset) - Math.abs(b.start - offset))[0] ?? null;
}

/** "Import class" of the demo's context actions (`code_context_actions`). */
export function mockImportClassAction(path: string, text: string, offset: number): ContextAction | null {
  if (!isPhp(path)) return null;
  const u = mockUnresolved(text).find((x) => x.start <= offset && offset <= x.end && x.candidates.length);
  if (!u) return null;
  const base = { title: "Import class", kind: "quickFix" as const, source: "php", isPreferred: false, needsResolve: false };
  if (u.candidates.length > 1) {
    return { ...base, action: "PhpImportClass", data: { start: u.start, end: u.end, name: u.name, doc: u.doc, candidates: u.candidates } };
  }
  const edits = referenceEdits(text, u.start, u.end, u.candidates[0]);
  const pos = (o: number) => {
    const before = text.slice(0, o).split("\n");
    return { line: before.length - 1, character: before[before.length - 1].length };
  };
  const lsp: TextEditLsp[] = edits.map((e) => ({ range: { start: pos(e.start), end: pos(e.end) }, newText: e.newText }));
  return { ...base, edit: { changes: [{ kind: "edit", path, edits: lsp }] } };
}

/** Removed `use` lines and whether the kept ones moved. */
function optimizeReport(text: string): { next: string; removed: number; rearranged: boolean } {
  const next = mockOptimizeImports(text, true);
  const count = (t: string) => (t.match(/^use [^;]+;$/gm) ?? []).length;
  const removed = count(text) - count(next);
  return { next, removed, rearranged: next !== text && (removed === 0 || mockOptimizeImports(text, false) !== next) };
}

/** JavaScript / TypeScript: the demo "TypeScript server" sorts the import lines. */
function organizeWeb(text: string): string {
  const lines = text.split("\n");
  const imports = lines.filter((l) => /^import .* from ["'][^"']+["'];?$/.test(l));
  if (imports.length < 2) return text;
  const sorted = [...imports].sort((a, b) => a.localeCompare(b));
  let i = 0;
  return lines.map((l) => (imports.includes(l) ? sorted[i++] : l)).join("\n");
}

registerMock("imports_optimize", async ({ path, content }): Promise<OptimizeAnswer> => {
  const text = String(content);
  const wasm = await forgeWasm();
  if (isPhp(String(path))) {
    const r = optimizeReport(text);
    return callWasm<OptimizeAnswer>(wasm.imports_optimize_answer, { old: text, new: r.next, removed: r.removed, rearranged: r.rearranged });
  }
  const next = organizeWeb(text);
  return callWasm<OptimizeAnswer>(wasm.imports_optimize_answer, { old: text, new: next, removed: 0, rearranged: next !== text });
});

registerMock("imports_optimize_paths", async (args): Promise<FolderOptimizeAnswer> => {
  const { paths, includeSubdirectories, onlyVcsChanged, buffers } = args as {
    paths: string[];
    includeSubdirectories: boolean;
    onlyVcsChanged: boolean;
    buffers?: Record<string, string>;
  };
  const all = Object.keys(mockFiles).filter((f) =>
    paths.some((p) => {
      if (!p || f === p) return !p ? includeSubdirectories || !f.includes("/") : true;
      if (!f.startsWith(`${p}/`)) return false;
      return includeSubdirectories || !f.slice(p.length + 1).includes("/");
    }),
  );
  let changed: string[] | null = null;
  if (onlyVcsChanged) {
    const status = await invoke<{ entries: { path: string }[] }>("git_status").catch(() => ({ entries: [] }));
    changed = status.entries.map((e) => e.path);
  }
  const wasm = await forgeWasm();
  const files = await callWasm<string[]>(wasm.imports_targets, { files: all, changed });
  const edit: WorkspaceEdit = { changes: [] };
  let removed = 0;
  for (const path of files) {
    const text = buffers?.[path] ?? mockFiles[path];
    const next = isPhp(path) ? optimizeReport(text) : { next: organizeWeb(text), removed: 0 };
    if (next.next === text) continue;
    removed += next.removed;
    const e = await callWasm<TextEditLsp | null>(wasm.imports_minimal_edit, { old: text, new: next.next });
    if (e) edit.changes.push({ kind: "edit", path, edits: [e] });
  }
  return { edit, files: files.length, changed: edit.changes.length, message: wasm.imports_folder_summary(files.length, edit.changes.length, removed) };
});

registerMock("php_import_hint", ({ path, content, offset }) => (isPhp(String(path)) ? hintAt(String(content), Number(offset)) : null));

registerMock("php_import_reference", ({ content, start, end, fqn }) => referenceEdits(String(content), Number(start), Number(end), String(fqn)));

registerMock("php_imports_on_the_fly", ({ content, request }): OnTheFlyAnswer => {
  const text = String(content);
  const req = request as OnTheFlyRequest;
  const answer: OnTheFlyAnswer = { edits: [], added: [], removed: [] };
  if (req.optimize) {
    const uses = [...text.matchAll(/^use ([\w\\]+)(?: as (\w+))?;\n/gm)];
    const caretInImports = uses.some((u) => req.caret >= u.index! && req.caret <= u.index! + u[0].length);
    if (!caretInImports) {
      for (const u of uses) {
        const name = u[2] ?? short(u[1]);
        const rest = text.slice(0, u.index!) + text.slice(u.index! + u[0].length);
        if (new RegExp(`\\b${name}\\b`).test(rest)) continue;
        answer.edits.push({ start: u.index!, end: u.index! + u[0].length, newText: "" });
        answer.removed.push(u[1]);
      }
      if (answer.edits.length) return answer;
    }
  }
  if (req.add) {
    for (const u of mockUnresolved(text)) {
      if (u.candidates.length !== 1 || (u.start <= req.caret && req.caret <= u.end) || answer.added.includes(u.candidates[0])) continue;
      const e = importEdit(text, u.candidates[0]);
      if (!e) continue;
      answer.edits.push(e);
      answer.added.push(u.candidates[0]);
    }
  }
  return answer;
});

registerMock("code_imports_on_the_fly", () => null);
