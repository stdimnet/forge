/**
 * Browser-mode Extract Interface / Superclass, Pull Members Up / Push Members Down and
 * Refactor This for PHP (the desktop app computes them in Rust, see
 * app/src-tauri/src/commands/refactor_hierarchy.rs). The tree-sitter grammar is not part of
 * the WebAssembly build, so these mocks approximate the refactorings on the in-memory demo
 * project with the regex PHP parser: enough to try the dialogs, the preview and undo. Only
 * the in-memory demo files change.
 */
import { registerMock } from "@/ipc";
import type { LspPosition } from "@/ipc/lsp";
import type {
  ClassMember,
  ExtractInterfaceOptions,
  ExtractSuperclassOptions,
  HierarchyClass,
  HierarchyInfo,
  HierarchyRefactoring,
  PullUpOptions,
  PushDownOptions,
  RefactorAnswer,
  RefactorThisAnswer,
} from "@/ipc/refactor";
import { offsetOf, positionOf } from "./codeintelMock";
import { CANNOT, Plan, allClasses, dirOf, isPhp, lineRange, qualify, textOf, type Buffers, type Edit } from "./moveMock";
import { parsePhp, type MockClass, type MockMember } from "./phpMockParser";
import { mockFiles } from "./workspaceMock";
import { isWebScript, webRefactorThis } from "./extractComponentMock";

const idOf = (m: MockMember) => (m.kind === "method" ? `${m.name}()` : m.kind === "property" ? `$${m.name}` : m.name);

function membersOf(text: string, cls: MockClass): { member: ClassMember; mock: MockMember }[] {
  const list = cls.members.filter((m) => !m.promoted && ["method", "property", "constant"].includes(m.kind));
  const ids = list.map(idOf);
  return list.map((m) => {
    const code = text.slice(m.start, m.end);
    const mods = `${m.visibility} ${m.modifiers.isAbstract ? "abstract " : ""}${m.modifiers.isStatic ? "static " : ""}`;
    const signature =
      m.kind === "method"
        ? `${mods}function ${m.name}(${m.params ?? ""})${m.type ? `: ${m.type}` : ""}`
        : m.kind === "constant"
          ? `${m.visibility} const ${m.name} = ${m.value ?? ""}`
          : `${mods}${m.type ? `${m.type} ` : ""}$${m.name}${m.value ? ` = ${m.value}` : ""}`;
    const uses = new Set<string>();
    for (const u of code.matchAll(/\$this->(\w+)\s*(\()?/g)) uses.add(u[2] ? `${u[1]}()` : `$${u[1]}`);
    for (const u of code.matchAll(/\b(?:self|static)::(\$?\w+)\s*(\()?/g)) uses.add(u[2] ? `${u[1]}()` : u[1]);
    const id = idOf(m);
    return {
      mock: m,
      member: {
        id,
        name: m.name,
        kind: m.kind as ClassMember["kind"],
        signature,
        visibility: m.visibility,
        isStatic: !!m.modifiers.isStatic,
        isAbstract: !!m.modifiers.isAbstract,
        constructor: m.name === "__construct",
        uses: [...uses].filter((u) => u !== id && ids.includes(u)),
      },
    };
  });
}

/** A class name as written in a file, fully qualified. */
function resolve(text: string, name: string): string {
  if (name.startsWith("\\")) return name.slice(1);
  const file = parsePhp(text);
  const [first, ...rest] = name.split("\\");
  const used = file.uses.get(first);
  if (used) return [used, ...rest].join("\\");
  return qualify(file.namespace, name);
}

function classAtCaret(text: string, at: number): MockClass | undefined {
  const file = parsePhp(text);
  return file.classes.find((c) => c.start <= at && at <= c.close + 1) ?? (file.classes.length === 1 ? file.classes[0] : undefined);
}

function supertypes(path: string, cls: MockClass, buffers: Buffers): HierarchyClass[] {
  const text = textOf(path, buffers) ?? "";
  const names = [...(cls.kind === "interface" ? [] : cls.extends ? [cls.extends] : []), ...cls.implements];
  const out: HierarchyClass[] = [];
  for (const n of names) {
    const fqn = resolve(text, n).toLowerCase();
    const found = allClasses(buffers).find((c) => c.cls.fqn.toLowerCase() === fqn);
    if (found) out.push({ name: found.cls.name, fqn: found.cls.fqn, kind: found.cls.kind === "interface" ? "interface" : "class", path: found.path });
  }
  return out;
}

function subclasses(cls: MockClass, buffers: Buffers): HierarchyClass[] {
  return allClasses(buffers)
    .filter(({ path, cls: c }) =>
      [c.extends, ...c.implements].some((n) => n && resolve(textOf(path, buffers) ?? "", n).toLowerCase() === cls.fqn.toLowerCase()),
    )
    .map(({ path, cls: c }) => ({ name: c.name, fqn: c.fqn, kind: c.kind === "interface" ? "interface" : "class", path }));
}

registerMock("refactor_hierarchy_prepare", ({ path, position, content, refactoring, buffers }): HierarchyInfo => {
  const text = String(content);
  const at = offsetOf(text, position as LspPosition);
  const cls = classAtCaret(text, at);
  if (!cls) throw new Error(`${CANNOT} Caret should be positioned inside the class to refactor`);
  const kind = refactoring as HierarchyRefactoring;
  const members = membersOf(text, cls);
  const targets = kind === "pullUp" ? supertypes(String(path), cls, buffers as Buffers) : kind === "pushDown" ? subclasses(cls, buffers as Buffers) : [];
  if (kind === "pullUp" && !targets.length) throw new Error(`${CANNOT} Class ${cls.name} has no superclass or interface in the project`);
  return {
    path: String(path),
    position: positionOf(text, cls.offset),
    directory: dirOf(String(path)),
    name: cls.name,
    fqn: cls.fqn,
    namespace: cls.namespace,
    kind: (["interface", "trait", "enum"].includes(cls.kind) ? cls.kind : "class") as HierarchyInfo["kind"],
    isAbstract: !!cls.modifiers.isAbstract,
    parent: cls.extends ? resolve(text, cls.extends) : null,
    interfaces: cls.implements.map((n) => resolve(text, n)),
    members: members.map((m) => m.member),
    selected: members.find((m) => m.mock.start <= at && at <= m.mock.end)?.member.id ?? null,
    targets,
  };
});

interface Args<O> {
  path: string;
  position: LspPosition;
  options: O;
  buffers: Buffers;
}

function load(path: string, position: LspPosition, plan: Plan) {
  const text = plan.text(path);
  const cls = parsePhp(text).classes.find((c) => c.offset === offsetOf(text, position));
  if (!cls) throw new Error(`${CANNOT} The class cannot be found`);
  return { text, cls, members: membersOf(text, cls) };
}

/** A member's code without its line break, as it goes into another class body. */
function codeOf(text: string, m: MockMember): string {
  const [s, e] = lineRange(text, m.start, m.end);
  return text.slice(s, e).replace(/^\n+|\n+$/g, "");
}

function declaration(text: string, m: MockMember, abstract: boolean): string {
  const code = codeOf(text, m);
  const brace = code.indexOf("{", code.indexOf("function"));
  let head = (brace < 0 ? code.replace(/;\s*$/, "") : code.slice(0, brace)).trimEnd();
  if (abstract) head = head.replace(/(^|\n)(\s*)((?:final\s+)?(?:public|protected|private))/, "$1$2abstract $3").replace("private", "protected");
  else head = head.replace(/\b(protected|private|final|abstract)\s+/g, "").replace(/(^|\n)(\s*)function/, "$1$2public function");
  return `${head};`;
}

function createFile(plan: Plan, path: string, text: string) {
  plan.edit.changes.push({ kind: "create", path, overwrite: false });
  plan.edit.changes.push({ kind: "edit", path, edits: [{ range: { start: { line: 0, character: 0 }, end: { line: 0, character: 0 } }, newText: text }] });
}

function newFile(kind: "interface" | "class", name: string, ns: string, body: string, head = "", abstract = false): string {
  return `<?php\n\n${ns ? `namespace ${ns};\n\n` : ""}${head}${abstract ? "abstract " : ""}${kind} ${name}\n{\n${body}\n}\n`;
}

function checkNew(name: string, ns: string, dir: string, buffers: Buffers): string {
  if (!/^[A-Za-z_]\w*$/.test(name)) throw new Error(`${CANNOT} '${name}' is not a valid identifier`);
  const fqn = qualify(ns, name);
  if (allClasses(buffers).some((c) => c.cls.fqn.toLowerCase() === fqn.toLowerCase())) throw new Error(`${CANNOT} Class ${fqn} already exists`);
  const path = `${dir ? `${dir}/` : ""}${name}.php`;
  if (mockFiles[path] !== undefined) throw new Error(`${CANNOT} File ${path} already exists`);
  return path;
}

/** The class header edit: `implements X` added or `extends X` set. */
function headerEdit(text: string, cls: MockClass, word: "implements" | "extends", name: string): Edit {
  const header = text.slice(cls.offset, cls.open);
  if (word === "implements" && cls.implements.length) {
    const end = cls.offset + header.trimEnd().length;
    return { start: end, end, text: `, ${name}` };
  }
  if (word === "extends" && cls.extends) {
    const at = cls.offset + header.indexOf(cls.extends, header.indexOf("extends"));
    return { start: at, end: at + cls.extends.length, text: name };
  }
  const after = cls.offset + (word === "implements" ? header.trimEnd().length : cls.name.length);
  return { start: after, end: after, text: ` ${word} ${name}` };
}

function importEdit(text: string, fqn: string, ns: string): Edit[] {
  const fqnNs = fqn.includes("\\") ? fqn.slice(0, fqn.lastIndexOf("\\")) : "";
  if (fqnNs.toLowerCase() === ns.toLowerCase()) return [];
  const m = /namespace\s+[\w\\]+\s*;\n/.exec(text);
  if (!m) return [];
  const at = m.index + m[0].length;
  return [{ start: at, end: at, text: `\nuse ${fqn};\n` }];
}

registerMock("refactor_extract_interface", async (args): Promise<RefactorAnswer> => {
  const { path, position, options, buffers } = args as unknown as Args<ExtractInterfaceOptions>;
  const plan = new Plan(buffers);
  const { text, cls, members } = load(path, position, plan);
  const newPath = checkNew(options.name, options.namespace, options.directory, buffers);
  const chosen = members.filter((m) => options.members.includes(m.member.id));
  const body = [
    ...chosen.filter((m) => m.member.kind === "constant").map((m) => codeOf(text, m.mock)),
    ...chosen.filter((m) => m.member.kind === "method").map((m) => declaration(text, m.mock, false)),
  ].join("\n\n");
  createFile(plan, newPath, newFile("interface", options.name, options.namespace, body));
  const edits: Edit[] = [headerEdit(text, cls, "implements", options.name), ...importEdit(text, qualify(options.namespace, options.name), cls.namespace)];
  for (const m of chosen.filter((x) => x.member.kind === "constant")) {
    const [s, e] = lineRange(text, m.mock.start, m.mock.end);
    edits.push({ start: s, end: e, text: "" });
  }
  plan.edits(path, edits);
  return plan.answer();
});

registerMock("refactor_extract_superclass", async (args): Promise<RefactorAnswer> => {
  const { path, position, options, buffers } = args as unknown as Args<ExtractSuperclassOptions>;
  const plan = new Plan(buffers);
  const { text, cls, members } = load(path, position, plan);
  const newPath = checkNew(options.name, options.namespace, options.directory, buffers);
  const abstract = new Set(options.members.filter((c) => c.makeAbstract).map((c) => c.id));
  const chosen = members.filter((m) => options.members.some((c) => c.id === m.member.id));
  const edits: Edit[] = [];
  const body = chosen
    .map((m) => {
      if (abstract.has(m.member.id)) return declaration(text, m.mock, true);
      const [s, e] = lineRange(text, m.mock.start, m.mock.end);
      edits.push({ start: s, end: e, text: "" });
      return codeOf(text, m.mock);
    })
    .join("\n\n");
  const parent = cls.extends ? resolve(text, cls.extends) : null;
  const head = parent && parent.includes("\\") ? `use ${parent};\n\n` : "";
  const decl = parent ? `${options.name} extends ${parent.split("\\").pop()}` : options.name;
  createFile(plan, newPath, newFile("class", decl, options.namespace, body, head, abstract.size > 0));
  edits.push(headerEdit(text, cls, "extends", options.name), ...importEdit(text, qualify(options.namespace, options.name), cls.namespace));
  plan.edits(path, edits);
  return plan.answer();
});

/** Members into the body of another class (before its closing brace). */
function insertInto(plan: Plan, path: string, cls: MockClass, code: string): Edit {
  const t = plan.text(path);
  const closeLine = t.lastIndexOf("\n", cls.close - 1) + 1;
  const empty = t.slice(cls.open + 1, cls.close).trim() === "";
  return { start: closeLine, end: closeLine, text: `${empty ? "" : "\n"}${code}\n` };
}

registerMock("refactor_pull_up", async (args): Promise<RefactorAnswer> => {
  const { path, position, options, buffers } = args as unknown as Args<PullUpOptions>;
  const plan = new Plan(buffers);
  const { text, members } = load(path, position, plan);
  const dest = allClasses(buffers).find((c) => c.cls.fqn.toLowerCase() === options.target.toLowerCase());
  if (!dest) throw new Error(`${CANNOT} Class ${options.target} does not exist`);
  const iface = dest.cls.kind === "interface";
  const chosen = members.filter((m) => options.members.some((c) => c.id === m.member.id));
  const moved = new Set(chosen.map((m) => m.member.id));
  const edits: Edit[] = [];
  const parts: string[] = [];
  let needsAbstract = false;
  for (const m of chosen) {
    const abstract = iface ? m.member.kind === "method" : options.members.some((c) => c.id === m.member.id && c.makeAbstract);
    for (const u of abstract ? [] : m.member.uses.filter((x) => !moved.has(x)))
      plan.conflicts.push({ message: `${m.member.kind[0].toUpperCase()}${m.member.kind.slice(1)} ${m.member.id} uses ${u}, which is not pulled up to ${dest.cls.name}`, path, line: 0 });
    if (abstract) {
      parts.push(declaration(text, m.mock, !iface));
      needsAbstract ||= !iface;
    } else {
      const [s, e] = lineRange(text, m.mock.start, m.mock.end);
      edits.push({ start: s, end: e, text: "" });
      parts.push(codeOf(text, m.mock).replace(/\bprivate\b/, "protected"));
    }
  }
  plan.edits(path, edits);
  const destEdits = [insertInto(plan, dest.path, dest.cls, parts.join("\n\n"))];
  if (needsAbstract && !dest.cls.modifiers.isAbstract) {
    const t = plan.text(dest.path);
    const kw = t.lastIndexOf("class", dest.cls.offset);
    destEdits.push({ start: kw, end: kw, text: "abstract " });
  }
  plan.edits(dest.path, destEdits);
  return plan.answer();
});

registerMock("refactor_push_down", async (args): Promise<RefactorAnswer> => {
  const { path, position, options, buffers } = args as unknown as Args<PushDownOptions>;
  const plan = new Plan(buffers);
  const { text, cls, members } = load(path, position, plan);
  const subs = subclasses(cls, buffers);
  if (!subs.length) throw new Error(`${CANNOT} Class ${cls.name} has no subclasses to push members down to`);
  const chosen = members.filter((m) => options.members.some((c) => c.id === m.member.id));
  const edits: Edit[] = [];
  for (const m of chosen) {
    const [s, e] = lineRange(text, m.mock.start, m.mock.end);
    const keep = options.members.some((c) => c.id === m.member.id && c.makeAbstract);
    edits.push({ start: s, end: e, text: keep ? `${declaration(text, m.mock, true)}\n` : "" });
  }
  plan.edits(path, edits);
  for (const sub of subs) {
    const sc = parsePhp(plan.text(sub.path)).classes.find((c) => c.fqn === sub.fqn);
    if (!sc) continue;
    const code = chosen
      .filter((m) => !sc.members.some((x) => x.kind === m.mock.kind && x.name === m.mock.name))
      .map((m) => codeOf(text, m.mock))
      .join("\n\n");
    if (code) plan.edits(sub.path, [insertInto(plan, sub.path, sc, code)]);
  }
  return plan.answer();
});

// ---------- Refactor This ----------

/** A rough version of Rust's `refactorings_at` for the demo files. */
export function phpRefactorings(text: string, start: number, end: number): string[] {
  const selected = text.slice(start, end).trim();
  if (selected) return ["IntroduceVariable", "IntroduceConstant", "IntroduceField", "IntroduceParameter", "ExtractMethod"];
  const cls = classAtCaret(text, start);
  const pullUp = cls && (cls.extends || cls.implements.length) ? ["MembersPullUp"] : [];
  const classSet = ["RenameElement", "Move", "CopyElement", "SafeDelete", "ExtractInterface", "ExtractSuperclass", ...pullUp, "MemberPushDown"];
  if (!cls) return [];
  if (start <= cls.open) return classSet;
  const member = cls.members.find((m) => !m.promoted && m.start <= start && start <= m.end);
  if (!member) return classSet;
  const bodyOpen = member.kind === "method" ? text.indexOf("{", text.indexOf(")", member.offset)) : -1;
  if (member.kind === "method" && bodyOpen >= 0 && start > bodyOpen) {
    const word = /[\w$]$/.test(text.slice(0, start)) || /^[\w$]/.test(text.slice(start));
    return ["IntroduceVariable", "IntroduceConstant", "IntroduceField", "IntroduceParameter", ...(word ? ["RenameElement", "Inline"] : [])];
  }
  if (member.kind === "method")
    return [
      "RenameElement",
      "ChangeSignature",
      ...(member.modifiers.isStatic ? ["Move"] : []),
      "SafeDelete",
      "Inline",
      ...pullUp,
      "MemberPushDown",
      ...(member.modifiers.isStatic ? [] : ["MakeStatic"]),
    ];
  return ["RenameElement", "SafeDelete", ...pullUp, "MemberPushDown"];
}

registerMock("code_refactor_this", async (args): Promise<RefactorThisAnswer> => {
  const { path, line, character, endLine, endCharacter, content } = args as {
    path: string;
    line: number;
    character: number;
    endLine: number;
    endCharacter: number;
    content?: string;
  };
  const text = content ?? mockFiles[path] ?? "";
  if (isWebScript(path))
    return webRefactorThis(path, text, { start: { line, character }, end: { line: endLine, character: endCharacter } });
  if (!isPhp(path)) return { actions: null, lsp: [] };
  const a = offsetOf(text, { line, character });
  const b = offsetOf(text, { line: endLine, character: endCharacter });
  return { actions: phpRefactorings(text, Math.min(a, b), Math.max(a, b)), lsp: [] };
});
