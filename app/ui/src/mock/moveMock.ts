/**
 * Browser-mode Move, Copy, Safe Delete, Change Signature and Make Static (the desktop app
 * computes them in Rust from the PHP index, see app/src-tauri/src/commands/refactor_move.rs).
 * The tree-sitter grammar is not part of the WebAssembly build, so these mocks approximate
 * the refactorings on the in-memory demo project with text heuristics: enough to try the
 * dialogs, the preview and undo. Usages come from the Find Usages mock; previews run the
 * Rust code (`forge-codeintel`) as WebAssembly. Only the in-memory demo files change.
 */
import { invoke, registerMock } from "@/ipc";
import type { FilePreview, TextEditLsp, UsagesAnswer, WorkspaceEdit } from "@/ipc/codeintel";
import type { LspPosition } from "@/ipc/lsp";
import type {
  ChangeSignatureOptions,
  ClassTarget,
  MakeStaticInfo,
  MakeStaticOptions,
  MoveFilesAnswer,
  MovePrepareAnswer,
  PathMove,
  RefactorAnswer,
  SafeDeleteAnswer,
  SignatureInfo,
  SignatureParam,
} from "@/ipc/refactor";
import { callWasm, forgeWasm } from "@/wasm";
import { offsetOf, positionOf } from "./codeintelMock";
import { parsePhp, type MockClass, type MockMember } from "./phpMockParser";
import { mockFiles } from "./workspaceMock";

const CANNOT = "Cannot perform refactoring.";
const isPhp = (p: string) => /\.(php|phtml|inc)$/i.test(p) && !p.endsWith(".blade.php");
const dirOf = (p: string) => (p.includes("/") ? p.slice(0, p.lastIndexOf("/")) : "");
const qualify = (ns: string, name: string) => (ns ? `${ns}\\${name}` : name);
const escape = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

type Buffers = Record<string, string> | undefined;
const textOf = (path: string, buffers: Buffers) => buffers?.[path] ?? mockFiles[path];

interface Edit {
  start: number;
  end: number;
  text: string;
}

/** Edits per file (byte offsets = JS offsets here) and file operations, in order. */
class Plan {
  edit: WorkspaceEdit = { changes: [] };
  conflicts: RefactorAnswer["conflicts"] = [];
  constructor(private buffers: Buffers) {}

  text(path: string): string {
    return textOf(path, this.buffers) ?? "";
  }

  edits(path: string, edits: Edit[], source = path): void {
    if (!edits.length) return;
    const text = this.text(source);
    const lsp: TextEditLsp[] = [...edits]
      .sort((a, b) => a.start - b.start)
      .map((e) => ({ range: { start: positionOf(text, e.start), end: positionOf(text, e.end) }, newText: e.text }));
    this.edit.changes.push({ kind: "edit", path, edits: lsp });
  }

  async answer(sources: (p: string) => string = (p) => p): Promise<RefactorAnswer> {
    const files: Record<string, string> = {};
    for (const c of this.edit.changes) {
      for (const p of c.kind === "rename" ? [c.from] : [c.path]) {
        const t = textOf(sources(p), this.buffers);
        if (t !== undefined) files[p] = t;
      }
    }
    const wasm = await forgeWasm();
    const preview = await callWasm<FilePreview[]>(wasm.codeintel_preview_edit, { edit: this.edit, files });
    return { edit: this.edit, preview, conflicts: this.conflicts, nameRanges: [], skipped: 0 };
  }
}

function classes(path: string, buffers: Buffers): MockClass[] {
  const t = textOf(path, buffers);
  return t === undefined ? [] : parsePhp(t).classes;
}

function allClasses(buffers: Buffers): { path: string; cls: MockClass }[] {
  return Object.keys(mockFiles)
    .filter(isPhp)
    .flatMap((path) => classes(path, buffers).map((cls) => ({ path, cls })));
}

/** Find Usages through the Find Usages mock, as `[path, offset, length]`. */
async function usagesAt(path: string, offset: number, buffers: Buffers): Promise<{ path: string; start: number; end: number }[]> {
  const text = textOf(path, buffers) ?? "";
  const p = positionOf(text, offset);
  const answer = await invoke<UsagesAnswer>("code_find_usages", { path, line: p.line, character: p.character, content: text, buffers });
  const out: { path: string; start: number; end: number }[] = [];
  for (const g of answer.byFile.groups) {
    for (const f of g.files) {
      const t = textOf(f.path, buffers) ?? "";
      for (const u of f.usages) {
        const start = offsetOf(t, { line: u.line, character: u.column });
        out.push({ path: f.path, start, end: offsetOf(t, { line: u.endLine, character: u.endColumn }) });
      }
    }
  }
  return out;
}

function lineRange(text: string, start: number, end: number): [number, number] {
  const ls = text.lastIndexOf("\n", start - 1) + 1;
  const docStart = (() => {
    const before = text.slice(0, ls).trimEnd();
    if (!before.endsWith("*/")) return ls;
    const open = before.lastIndexOf("/**");
    return open < 0 ? ls : text.lastIndexOf("\n", open - 1) + 1;
  })();
  let le = text.indexOf("\n", end);
  le = le < 0 ? text.length : le + 1;
  // One blank line goes with it.
  if (text.slice(le).startsWith("\n") && text.slice(0, docStart).endsWith("\n\n")) le += 1;
  return [docStart, le];
}

/** `use App\Models\User;` -> new FQN, `\App\Models\User` -> `\New`, bare names -> import. */
function retargetEdits(text: string, oldFqn: string, newFqn: string, spans: { start: number; end: number }[]): Edit[] {
  const edits: Edit[] = [];
  let needsImport = false;
  const ns = /namespace\s+([\w\\]+)\s*;/.exec(text)?.[1] ?? "";
  for (const s of spans) {
    let start = s.start;
    while (start > 0 && /[\w\\]/.test(text[start - 1])) start--;
    const written = text.slice(start, s.end);
    if (written.includes("\\")) {
      const lead = written.startsWith("\\") ? "\\" : "";
      if (written.replace(/^\\/, "").toLowerCase() === oldFqn.toLowerCase()) edits.push({ start, end: s.end, text: lead + newFqn });
      else if (!lead) edits.push({ start, end: s.end, text: `\\${newFqn}` });
    } else if (ns.toLowerCase() === dirNs(oldFqn).toLowerCase() && dirNs(newFqn).toLowerCase() !== ns.toLowerCase()) {
      needsImport = true;
    }
  }
  if (needsImport && !new RegExp(`use\\s+${escape(newFqn)}\\s*;`).test(text)) {
    const m = /namespace\s+[\w\\]+\s*;\n/.exec(text);
    if (m) edits.push({ start: m.index + m[0].length, end: m.index + m[0].length, text: `\nuse ${newFqn};\n` });
  }
  return edits;
}

const dirNs = (fqn: string) => (fqn.includes("\\") ? fqn.slice(0, fqn.lastIndexOf("\\")) : "");

function namespaceEdit(text: string, newNs: string): Edit[] {
  const m = /namespace\s+([\w\\]+)\s*;/.exec(text);
  if (!m) return [];
  const at = m.index + m[0].indexOf(m[1]);
  return [{ start: at, end: at + m[1].length, text: newNs }];
}

/** The class Move / Copy works on, or static members at the caret. */
registerMock("refactor_move_prepare", ({ path, position, content, copy }): MovePrepareAnswer => {
  const text = String(content);
  const at = offsetOf(text, position as LspPosition);
  const file = parsePhp(text);
  const cls = file.classes.find((c) => c.start <= at && at <= c.close) ?? (file.classes.length === 1 ? file.classes[0] : undefined);
  if (!cls) throw new Error(`${CANNOT} Caret should be positioned at the class or member to be moved`);
  const member = cls.members.find((m) => m.start <= at && at <= m.end);
  const statics = cls.members.filter((m) => m.kind === "constant" || (m.modifiers.isStatic && (m.kind === "method" || m.kind === "property")));
  if (!copy && member && statics.includes(member)) {
    return {
      kind: "members",
      members: {
        path: String(path),
        position: positionOf(text, cls.offset),
        className: cls.name,
        classFqn: cls.fqn,
        members: statics.map((m) => ({
          name: m.name,
          kind: m.kind as "method" | "property" | "constant",
          visibility: m.visibility,
          signature: text.slice(m.start, text.indexOf(m.kind === "method" ? ")" : ";", m.offset) + 1).trim().replace(/\s+/g, " "),
        })),
        selected: member.name,
      },
    };
  }
  const target: ClassTarget = {
    kind: "class",
    path: String(path),
    position: positionOf(text, cls.offset),
    name: cls.name,
    fqn: cls.fqn,
    namespace: cls.namespace,
    classKind: (["interface", "trait", "enum"].includes(cls.kind) ? cls.kind : "class") as ClassTarget["classKind"],
    sole: file.classes.length === 1,
    directory: dirOf(String(path)),
  };
  return { kind: "class", class: target };
});

registerMock("refactor_move_function", async (args): Promise<RefactorAnswer> => {
  const { path, position, target, buffers } = args as { path: string; position: LspPosition; target: string; buffers: Buffers };
  const plan = new Plan(buffers);
  const text = plan.text(path);
  const dest = mockFiles[target];
  if (dest === undefined) throw new Error(`${CANNOT} File ${target} does not exist`);
  const at = offsetOf(text, position);
  const start = text.lastIndexOf("function", at);
  let depth = 0;
  let end = text.indexOf("{", at);
  for (; end < text.length; end++) {
    if (text[end] === "{") depth++;
    if (text[end] === "}" && --depth === 0) break;
  }
  const [s, e] = lineRange(text, start, end + 1);
  plan.edits(path, [{ start: s, end: e, text: "" }]);
  plan.edits(target, [{ start: dest.length, end: dest.length, text: `\n${text.slice(s, e).trim()}\n` }]);
  return plan.answer();
});

registerMock("refactor_move_class", async (args): Promise<RefactorAnswer> => {
  const { path, position, targetNamespace, targetDirectory, buffers } = args as {
    path: string;
    position: LspPosition;
    targetNamespace: string;
    targetDirectory: string;
    buffers: Buffers;
  };
  const plan = new Plan(buffers);
  const text = plan.text(path);
  const cls = parsePhp(text).classes.find((c) => c.offset === offsetOf(text, position));
  if (!cls) throw new Error(`${CANNOT} Caret should be positioned at the class to be moved`);
  const newFqn = qualify(targetNamespace, cls.name);
  const newPath = `${targetDirectory ? `${targetDirectory}/` : ""}${cls.name}.php`;
  if (newFqn.toLowerCase() === cls.fqn.toLowerCase() && newPath === path) throw new Error(`${CANNOT} Class ${cls.name} is already in namespace ${targetNamespace}`);
  if (allClasses(buffers).some((c) => c.cls.fqn.toLowerCase() === newFqn.toLowerCase()))
    plan.conflicts.push({ message: `Class ${newFqn} already exists`, path: newPath, line: 0 });
  const usages = (await usagesAt(path, cls.offset, buffers)).filter((u) => u.path !== path);
  plan.edits(path, namespaceEdit(text, targetNamespace));
  if (newPath !== path) plan.edit.changes.push({ kind: "rename", from: path, to: newPath });
  const byFile = new Map<string, { start: number; end: number }[]>();
  for (const u of usages) byFile.set(u.path, [...(byFile.get(u.path) ?? []), u]);
  for (const [file, spans] of byFile) plan.edits(file, retargetEdits(plan.text(file), cls.fqn, newFqn, spans));
  return plan.answer();
});

registerMock("refactor_copy_class", async (args): Promise<RefactorAnswer> => {
  const { path, position, newName, targetNamespace, targetDirectory, buffers } = args as {
    path: string;
    position: LspPosition;
    newName: string;
    targetNamespace: string;
    targetDirectory: string;
    buffers: Buffers;
  };
  const plan = new Plan(buffers);
  const text = plan.text(path);
  const cls = parsePhp(text).classes.find((c) => c.offset === offsetOf(text, position));
  if (!cls) throw new Error(`${CANNOT} Caret should be positioned at the class to be copied`);
  const newPath = `${targetDirectory ? `${targetDirectory}/` : ""}${newName}.php`;
  if (mockFiles[newPath] !== undefined) throw new Error(`${CANNOT} File ${newPath} already exists`);
  let copy = text.replace(new RegExp(`\\b${escape(cls.name)}\\b`, "g"), newName);
  copy = copy.replace(/namespace\s+[\w\\]+\s*;/, `namespace ${targetNamespace};`);
  plan.edit.changes.push({ kind: "create", path: newPath, overwrite: false });
  plan.edit.changes.push({ kind: "edit", path: newPath, edits: [{ range: { start: { line: 0, character: 0 }, end: { line: 0, character: 0 } }, newText: copy }] });
  return plan.answer();
});

registerMock("refactor_move_members", async (args): Promise<RefactorAnswer> => {
  const { path, position, members, target, buffers } = args as {
    path: string;
    position: LspPosition;
    members: string[];
    target: string;
    buffers: Buffers;
  };
  const plan = new Plan(buffers);
  const text = plan.text(path);
  const cls = parsePhp(text).classes.find((c) => c.offset === offsetOf(text, position));
  const dest = allClasses(buffers).find((c) => c.cls.fqn.toLowerCase() === target.toLowerCase());
  if (!cls) throw new Error(`${CANNOT} The class cannot be found`);
  if (!dest) throw new Error(`${CANNOT} Class ${target} does not exist`);
  const moving = cls.members.filter((m) => members.includes(m.name));
  const removals: Edit[] = moving.map((m) => {
    const [s, e] = lineRange(text, m.start, m.end);
    return { start: s, end: e, text: "" };
  });
  const code = moving
    .map((m) => {
      const [s, e] = lineRange(text, m.start, m.end);
      return text.slice(s, e).replace(/^\n+|\n+$/g, "");
    })
    .join("\n\n");
  const usages: Record<string, Edit[]> = {};
  for (const m of moving) {
    for (const u of await usagesAt(path, m.offset, buffers)) {
      const t = plan.text(u.path);
      const before = t.slice(0, u.start).replace(/\$$/, "");
      const scope = /([\w\\]+)::$/.exec(before);
      if (!scope) continue;
      const s = before.length - scope[0].length;
      (usages[u.path] ??= []).push({ start: s, end: s + scope[1].length, text: dest.cls.name });
    }
  }
  const destText = plan.text(dest.path);
  const closeLine = destText.lastIndexOf("\n", dest.cls.close - 1) + 1;
  for (const [file, edits] of Object.entries(usages)) {
    if (file === path || file === dest.path) continue;
    plan.edits(file, edits);
  }
  plan.edits(path, [...removals, ...(usages[path] ?? []).filter((u) => !removals.some((r) => r.start <= u.start && u.end <= r.end))]);
  plan.edits(dest.path, [{ start: closeLine, end: closeLine, text: `${code}\n` }]);
  return plan.answer();
});

registerMock("refactor_move_files", async (args): Promise<MoveFilesAnswer> => {
  const { moves, searchForReferences, buffers } = args as { moves: PathMove[]; searchForReferences: boolean; buffers: Buffers };
  const files: PathMove[] = [];
  for (const m of moves) {
    for (const f of Object.keys(mockFiles)) {
      if (f === m.from || f.startsWith(`${m.from}/`)) files.push({ from: f, to: m.to + f.slice(m.from.length) });
    }
  }
  const plan = new Plan(buffers);
  const ns = (dir: string) => (dir === "app" || dir.startsWith("app/") ? ["App", ...dir.split("/").slice(1)].join("\\") : null);
  const back = new Map(files.map((f) => [f.to, f.from]));
  if (searchForReferences) {
    for (const f of files.filter((x) => isPhp(x.from))) {
      const text = plan.text(f.from);
      const oldNs = ns(dirOf(f.from));
      const newNs = ns(dirOf(f.to));
      const cls = parsePhp(text).classes[0];
      if (!cls || oldNs == null || newNs == null || oldNs !== cls.namespace || oldNs === newNs) continue;
      plan.edits(f.to, namespaceEdit(text, newNs), f.from);
      const newFqn = qualify(newNs, cls.name);
      const usages = await usagesAt(f.from, cls.offset, buffers);
      const byFile = new Map<string, { start: number; end: number }[]>();
      for (const u of usages.filter((u) => !files.some((x) => x.from === u.path))) byFile.set(u.path, [...(byFile.get(u.path) ?? []), u]);
      for (const [file, spans] of byFile) plan.edits(file, retargetEdits(plan.text(file), cls.fqn, newFqn, spans));
    }
  }
  return { answer: await plan.answer((p) => back.get(p) ?? p), files };
});

function deleteAnswerBase(kind: string, name: string, count: number, usages: UsagesAnswer, answer: RefactorAnswer, paths: string[] = []): SafeDeleteAnswer {
  const what = count === 1 ? "usage" : "usages";
  return {
    kind,
    name,
    message: `${kind} ${name} has ${count} ${what} that ${count === 1 ? "is" : "are"} not safe to delete.\nOf those 0 usages are in strings, comments, or non-code files.`,
    count,
    inComments: 0,
    usages,
    answer,
    paths,
  };
}

async function usageTree(locations: { path: string; start: number; end: number }[], title: string, buffers: Buffers): Promise<UsagesAnswer> {
  const usages = locations.map((u) => {
    const t = textOf(u.path, buffers) ?? "";
    return { path: u.path, range: { start: positionOf(t, u.start), end: positionOf(t, u.end) } };
  });
  const files: Record<string, string> = {};
  for (const u of usages) files[u.path] ??= textOf(u.path, buffers) ?? "";
  const wasm = await forgeWasm();
  const trees = await callWasm<Pick<UsagesAnswer, "byType" | "byFile">>(wasm.codeintel_group_usages, { usages, declaration: null, files });
  return { title, source: "index", declaration: null, ...trees, element: "class", scope: "Project Files" };
}

registerMock("refactor_safe_delete_prepare", async (args): Promise<SafeDeleteAnswer> => {
  const { path, position, content, buffers } = args as { path: string; position: LspPosition; content: string; buffers: Buffers };
  const text = String(content);
  const at = offsetOf(text, position);
  const cls = parsePhp(text).classes.find((c) => c.start <= at && at <= c.close);
  if (!cls) throw new Error(`${CANNOT} Caret should be positioned at the name of the element to be deleted`);
  const member = cls.members.find((m: MockMember) => m.start <= at && at <= m.end);
  const plan = new Plan({ ...buffers, [path]: text });
  if (member) {
    const kind = member.kind === "enumCase" ? "enum case" : member.kind;
    const name = member.kind === "method" ? `${member.name}()` : member.kind === "property" ? `$${member.name}` : member.name;
    const found = (await usagesAt(path, member.offset, { ...buffers, [path]: text })).filter((u) => !(u.path === path && member.start <= u.start && u.end <= member.end));
    const [s, e] = lineRange(text, member.start, member.end);
    plan.edits(path, [{ start: s, end: e, text: "" }]);
    return deleteAnswerBase(kind, name, found.length, await usageTree(found, name, buffers), await plan.answer());
  }
  const found = (await usagesAt(path, cls.offset, { ...buffers, [path]: text })).filter((u) => u.path !== path);
  plan.edit.changes.push({ kind: "delete", path });
  return deleteAnswerBase(cls.kind === "enum" ? "enum" : cls.kind, cls.name, found.length, await usageTree(found, cls.name, buffers), await plan.answer());
});

registerMock("refactor_safe_delete_files", async (args): Promise<SafeDeleteAnswer> => {
  const { paths, buffers } = args as { paths: string[]; buffers: Buffers };
  const all = Object.keys(mockFiles).filter((f) => paths.some((p) => f === p || f.startsWith(`${p}/`)));
  const found: { path: string; start: number; end: number }[] = [];
  for (const f of all.filter(isPhp)) {
    for (const c of classes(f, buffers)) found.push(...(await usagesAt(f, c.offset, buffers)).filter((u) => !all.includes(u.path)));
  }
  const plan = new Plan(buffers);
  const rest: string[] = [];
  for (const p of paths) {
    if (mockFiles[p] !== undefined) plan.edit.changes.push({ kind: "delete", path: p });
    else rest.push(p);
  }
  const single = paths.length === 1;
  const kind = single ? (mockFiles[paths[0]] !== undefined ? "file" : "directory") : "files";
  const name = single ? paths[0].split("/").pop()! : `${paths.length} items`;
  return deleteAnswerBase(kind, name, found.length, await usageTree(found, name, buffers), await plan.answer(), rest);
});

/** Split a parameter list at its top-level commas. */
function splitList(s: string): string[] {
  const out: string[] = [];
  let depth = 0;
  let cur = "";
  for (const c of s) {
    if ("([{".includes(c)) depth++;
    if (")]}".includes(c)) depth--;
    if (c === "," && depth === 0) {
      out.push(cur.trim());
      cur = "";
    } else cur += c;
  }
  if (cur.trim()) out.push(cur.trim());
  return out;
}

function parseParam(p: string): SignatureParam {
  const m = /^((?:(?:public|protected|private|readonly)\s+)*)([^$&.]*?)\s*(&)?\s*(\.\.\.)?\$(\w+)(?:\s*=\s*(.+))?$/s.exec(p.trim());
  if (!m) return { name: p.replace(/\W/g, "") || "param" };
  return {
    promotion: m[1].trim() || null,
    typeHint: m[2].trim() || null,
    byRef: !!m[3],
    variadic: !!m[4],
    name: m[5],
    defaultValue: m[6]?.trim() ?? null,
  };
}

function methodAt(text: string, at: number): { cls: MockClass; m: MockMember } | null {
  for (const cls of parsePhp(text).classes) {
    const m = cls.members.find((x) => x.kind === "method" && x.start <= at && at <= x.end);
    if (m) return { cls, m };
  }
  return null;
}

function signatureOf(path: string, text: string, at: number): SignatureInfo {
  const found = methodAt(text, at);
  if (!found) throw new Error(`${CANNOT} Caret should be positioned at the name of the method or function to be refactored`);
  const { cls, m } = found;
  const open = text.indexOf("(", m.offset);
  let depth = 0;
  let close = open;
  for (; close < text.length; close++) {
    if (text[close] === "(") depth++;
    if (text[close] === ")" && --depth === 0) break;
  }
  const params = splitList(text.slice(open + 1, close)).map(parseParam);
  const body = text.slice(close, m.end);
  for (const p of params) p.used = new RegExp(`\\$${p.name}\\b`).test(body);
  const ret = /^\)\s*:\s*([^{;]+)/.exec(text.slice(close))?.[1].trim() ?? null;
  const info: SignatureInfo = {
    path,
    position: positionOf(text, m.offset),
    kind: "method",
    name: m.name,
    className: cls.name,
    visibility: m.visibility,
    isStatic: m.modifiers.isStatic ?? false,
    isAbstract: m.modifiers.isAbstract ?? false,
    constructor: m.name === "__construct",
    returnType: ret,
    parameters: params,
    signature: "",
  };
  info.signature = `${m.visibility} ${info.isStatic ? "static " : ""}function ${m.name}(${text.slice(open + 1, close).trim()})${ret ? `: ${ret}` : ""}`;
  return info;
}

registerMock("refactor_change_signature_prepare", ({ path, position, content }): SignatureInfo => {
  const text = String(content);
  return signatureOf(String(path), text, offsetOf(text, position as LspPosition));
});

registerMock("refactor_change_signature", async (args): Promise<RefactorAnswer> => {
  const { path, position, content, options, buffers } = args as {
    path: string;
    position: LspPosition;
    content?: string;
    options: ChangeSignatureOptions;
    buffers: Buffers;
  };
  const all = { ...buffers, ...(content !== undefined ? { [path]: content } : {}) };
  const plan = new Plan(all);
  const text = plan.text(path);
  const at = offsetOf(text, position);
  const info = signatureOf(path, text, at);
  const render = options.parameters
    .map((p) => {
      const old = p.original != null ? info.parameters[p.original] : undefined;
      return `${old?.promotion ? `${old.promotion} ` : ""}${p.typeHint ? `${p.typeHint} ` : ""}${p.byRef ? "&" : ""}${p.variadic ? "..." : ""}$${p.name}${p.defaultValue ? ` = ${p.defaultValue}` : ""}`;
    })
    .join(", ");
  const nameAt = offsetOf(text, info.position);
  const open = text.indexOf("(", nameAt);
  let close = open;
  for (let depth = 0; close < text.length; close++) {
    if (text[close] === "(") depth++;
    if (text[close] === ")" && --depth === 0) break;
  }
  const decl: Edit[] = [{ start: nameAt, end: nameAt + info.name.length, text: options.name }, { start: open + 1, end: close, text: render }];
  const ret = /^\)(\s*:\s*[^{;]+?)\s*[{;]/.exec(text.slice(close));
  if (ret) decl.push({ start: close + 1, end: close + 1 + ret[1].length, text: options.returnType ? `: ${options.returnType}` : "" });
  else if (options.returnType) decl.push({ start: close + 1, end: close + 1, text: `: ${options.returnType}` });
  const vis = /(public|protected|private)\s+(?:static\s+)?function/.exec(text.slice(text.lastIndexOf("\n", nameAt), nameAt));
  if (vis && options.visibility && vis[1] !== options.visibility) {
    const s = text.lastIndexOf("\n", nameAt) + vis.index;
    decl.push({ start: s, end: s + vis[1].length, text: options.visibility });
  }
  const byFile = new Map<string, Edit[]>([[path, decl]]);
  for (const u of await usagesAt(path, nameAt, all)) {
    const t = plan.text(u.path);
    if (t[u.end] !== "(") continue;
    let e = u.end;
    for (let depth = 0; e < t.length; e++) {
      if (t[e] === "(") depth++;
      if (t[e] === ")" && --depth === 0) break;
    }
    const args = splitList(t.slice(u.end + 1, e));
    const next = options.parameters
      .map((p) => (p.original != null ? args[p.original] : (p.callValue ?? undefined)))
      .filter((a): a is string => a !== undefined);
    const edits = byFile.get(u.path) ?? [];
    edits.push({ start: u.start, end: u.end, text: options.name }, { start: u.end + 1, end: e, text: next.join(", ") });
    byFile.set(u.path, edits);
  }
  for (const [file, edits] of byFile) plan.edits(file, edits);
  return plan.answer();
});

registerMock("refactor_make_static_prepare", ({ path, position, content }): MakeStaticInfo => {
  const text = String(content);
  const found = methodAt(text, offsetOf(text, position as LspPosition));
  if (!found) throw new Error(`${CANNOT} Caret should be positioned at the method to be made static`);
  if (found.m.modifiers.isStatic) throw new Error(`${CANNOT} The method is already static`);
  const body = text.slice(found.m.start, found.m.end);
  const fields = [...new Set([...body.matchAll(/\$this->(\w+)\b(?!\s*\()/g)].map((x) => x[1]))];
  return {
    path: String(path),
    position: positionOf(text, found.m.offset),
    name: found.m.name,
    className: found.cls.name,
    usesThis: body.includes("$this"),
    fields,
    needsObject: /\$this->\w+\s*\(|\$this(?!->)/.test(body),
    parameter: found.cls.name.charAt(0).toLowerCase() + found.cls.name.slice(1),
  };
});

registerMock("refactor_make_static", async (args): Promise<RefactorAnswer> => {
  const { path, position, content, options, buffers } = args as {
    path: string;
    position: LspPosition;
    content?: string;
    options: MakeStaticOptions;
    buffers: Buffers;
  };
  const all = { ...buffers, ...(content !== undefined ? { [path]: content } : {}) };
  const plan = new Plan(all);
  const text = plan.text(path);
  const nameAt = offsetOf(text, position);
  const found = methodAt(text, nameAt);
  if (!found) throw new Error(`${CANNOT} The method cannot be found`);
  const fnAt = text.lastIndexOf("function", nameAt);
  const edits: Edit[] = [{ start: fnAt, end: fnAt, text: "static " }];
  const param = options.objectParameter;
  const body = text.slice(found.m.start, found.m.end);
  if (param) {
    for (const m of body.matchAll(/\$this\b/g)) edits.push({ start: found.m.start + m.index!, end: found.m.start + m.index! + 5, text: `$${param}` });
  } else if (options.fieldParameters) {
    for (const m of body.matchAll(/\$this->(\w+)\b(?!\s*\()/g)) edits.push({ start: found.m.start + m.index!, end: found.m.start + m.index! + m[0].length, text: `$${m[1]}` });
  }
  const fields = [...new Set([...body.matchAll(/\$this->(\w+)\b(?!\s*\()/g)].map((x) => x[1]))];
  const added = param ? [`${found.cls.name} $${param}`] : options.fieldParameters ? fields.map((f) => `$${f}`) : [];
  const open = text.indexOf("(", nameAt);
  if (added.length) edits.push({ start: open + 1, end: open + 1, text: added.join(", ") + (text[open + 1] === ")" ? "" : ", ") });
  const byFile = new Map<string, Edit[]>([[path, edits]]);
  for (const u of await usagesAt(path, nameAt, all)) {
    const t = plan.text(u.path);
    const recv = /(\$\w+)(\?)?->$/.exec(t.slice(0, u.start));
    if (!recv) continue;
    const s = u.start - recv[0].length;
    const scope = recv[1] === "$this" ? "self" : found.cls.name;
    const extra = param ? [recv[1]] : options.fieldParameters ? fields.map((f) => `${recv[1]}->${f}`) : [];
    const list = byFile.get(u.path) ?? [];
    list.push({ start: s, end: u.start, text: `${scope}::` });
    if (extra.length) list.push({ start: u.end + 1, end: u.end + 1, text: extra.join(", ") + (t[u.end + 1] === ")" ? "" : ", ") });
    byFile.set(u.path, list);
  }
  for (const [file, e] of byFile) plan.edits(file, e);
  return plan.answer();
});

registerMock("fs_copy", ({ from, to }) => {
  const src = String(from);
  const dst = String(to);
  if (dst === src || dst.startsWith(`${src}/`)) throw new Error(`cannot copy ${src} into itself`);
  if (Object.keys(mockFiles).some((f) => f === dst || f.startsWith(`${dst}/`))) throw new Error(`${dst} already exists`);
  const list = Object.keys(mockFiles).filter((f) => f === src || f.startsWith(`${src}/`));
  if (!list.length) throw new Error(`${src} does not exist`);
  for (const f of list) mockFiles[dst + f.slice(src.length)] = mockFiles[f];
});
