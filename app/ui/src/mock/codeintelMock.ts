/**
 * Browser-mode code intelligence over the demo project (the desktop app answers from the
 * language servers and the tree-sitter PHP index, see app/src-tauri/src/commands/codeintel.rs):
 * Go to Declaration, Ctrl+hover info, Find / Show Usages, Implementation / Super Method /
 * Type Declaration, gutter markers and read-only PHP stubs. Name resolution uses the small
 * regex parser of the PHP mocks; grouping usages, applying and previewing edits and ordering
 * context actions run the Rust code (`forge-codeintel`) as WebAssembly.
 */
import { invoke, registerMock } from "@/ipc";
import type {
  AppliedFile,
  ContextAction,
  DeclarationAnswer,
  LibraryFile,
  LineMarkerInfo,
  NavigateAnswer,
  NavKind,
  NavTarget,
  SymbolHover,
  UsageLocation,
  UsageTree,
  UsagesAnswer,
  WorkspaceEdit,
} from "@/ipc/codeintel";
import type { EditorLocation, LspPosition } from "@/ipc/lsp";
import { callWasm, forgeWasm } from "@/wasm";
import { demoStubs, demoStubsDetail } from "./demoStubs";
import { parsePhp, classAt, memberType, type MockClass, type MockFile, type MockMember } from "./phpMockParser";
import { mockFiles } from "./workspaceMock";

const STUBS = "library://php-stubs/";

const isPhp = (p: string) => /\.(php|phtml|inc)$/i.test(p) && !p.endsWith(".blade.php");

type Buffers = Record<string, string> | undefined;

function textOf(path: string, buffers?: Buffers): string | undefined {
  if (buffers?.[path] !== undefined) return buffers[path];
  if (path.startsWith(STUBS)) return demoStubs[path.slice(STUBS.length)];
  return mockFiles[path];
}

export function offsetOf(text: string, pos: LspPosition): number {
  let line = 0;
  let i = 0;
  while (line < pos.line && i < text.length) {
    const nl = text.indexOf("\n", i);
    if (nl < 0) return text.length;
    i = nl + 1;
    line++;
  }
  const end = text.indexOf("\n", i);
  return Math.min(i + pos.character, end < 0 ? text.length : end);
}

export function positionOf(text: string, offset: number): LspPosition {
  const before = text.slice(0, offset);
  const line = before.split("\n").length - 1;
  return { line, character: offset - (before.lastIndexOf("\n") + 1) };
}

// ---- Parsed files ----

interface Decl {
  path: string;
  text: string;
  file: MockFile;
  cls: MockClass;
}

const parsed = new Map<string, { text: string; file: MockFile }>();

function parse(path: string, text: string): MockFile {
  const hit = parsed.get(path);
  if (hit && hit.text === text) return hit.file;
  const file = parsePhp(text);
  parsed.set(path, { text, file });
  return file;
}

function sources(buffers?: Buffers): { path: string; text: string; stub: boolean }[] {
  const out = Object.keys(mockFiles)
    .filter(isPhp)
    .sort()
    .map((path) => ({ path, text: textOf(path, buffers)!, stub: false }));
  for (const [rel, text] of Object.entries(demoStubs)) out.push({ path: STUBS + rel, text, stub: true });
  return out;
}

function allClasses(buffers?: Buffers): Decl[] {
  return sources(buffers).flatMap((s) => {
    const file = parse(s.path, s.text);
    return file.classes.map((cls) => ({ path: s.path, text: s.text, file, cls }));
  });
}

function findClass(fqn: string, buffers?: Buffers): Decl | undefined {
  const key = fqn.replace(/^\\/, "").toLowerCase();
  return allClasses(buffers).find((d) => d.cls.fqn.toLowerCase() === key);
}

function resolveName(name: string, file: MockFile): string {
  if (name.startsWith("\\")) return name.slice(1);
  const [head, ...rest] = name.split("\\");
  const imported = file.uses.get(head);
  if (imported) return [imported, ...rest].join("\\");
  return file.namespace ? `${file.namespace}\\${name}` : name;
}

/** The class and its ancestors (parents, interfaces), nearest first. */
function lineage(d: Decl, buffers?: Buffers): Decl[] {
  const out = [d];
  for (let i = 0; i < out.length && out.length < 32; i++) {
    const c = out[i];
    for (const name of [c.cls.extends, ...c.cls.implements].filter(Boolean) as string[]) {
      const found = findClass(resolveName(name, c.file), buffers);
      if (found && !out.some((x) => x.cls.fqn === found.cls.fqn)) out.push(found);
    }
  }
  return out;
}

function findMember(fqn: string, name: string, kinds: MockMember["kind"][], buffers?: Buffers) {
  const d = findClass(fqn, buffers);
  if (!d) return undefined;
  for (const c of lineage(d, buffers)) {
    const m = c.cls.members.find((x) => kinds.includes(x.kind) && (x.kind === "method" ? x.name.toLowerCase() === name.toLowerCase() : x.name === name));
    if (m) return { decl: c, member: m };
  }
  return undefined;
}

interface StubFunction {
  name: string;
  path: string;
  offset: number;
  signature: string;
}

function stubFunctions(): StubFunction[] {
  const out: StubFunction[] = [];
  for (const [rel, text] of Object.entries(demoStubs)) {
    for (const m of text.matchAll(/^function\s+(\w+)\s*\(([^)]*)\)\s*(?::\s*([^{;]+?))?\s*\{/gm)) {
      out.push({
        name: m[1],
        path: STUBS + rel,
        offset: m.index! + m[0].indexOf(m[1]),
        signature: `function ${m[1]}(${m[2]})${m[3] ? `: ${m[3].trim()}` : ""}`,
      });
    }
  }
  return out;
}

function stubConstants(): StubFunction[] {
  const out: StubFunction[] = [];
  for (const [rel, text] of Object.entries(demoStubs)) {
    for (const m of text.matchAll(/define\('(\w+)',\s*([^)]*)\)/g)) {
      out.push({ name: m[1], path: STUBS + rel, offset: m.index! + 8, signature: `const ${m[1]} = ${m[2].trim()}` });
    }
  }
  return out;
}

// ---- Resolution ----

type Sym =
  | { kind: "class"; fqn: string }
  | { kind: "member"; cls: string; name: string; member: MockMember["kind"] }
  | { kind: "function"; name: string }
  | { kind: "constant"; name: string }
  | { kind: "variable"; name: string; path: string; at: number };

interface Resolution {
  start: number;
  end: number;
  /** The caret is on the declaration itself. */
  decl: boolean;
  sym: Sym;
}

const IDENT = /[\w\\$]/;
const KEYWORDS = new Set(
  "abstract and array as break case catch class clone const continue declare default do echo else elseif empty enum extends final finally fn for foreach function global if implements include instanceof interface isset list match namespace new or print private protected public readonly require require_once return static switch throw trait try unset use var while yield int float string bool void mixed null true false never iterable callable object self parent".split(
    " ",
  ),
);

function wordAt(text: string, offset: number): [number, number] | null {
  let s = offset;
  let e = offset;
  while (s > 0 && IDENT.test(text[s - 1])) s--;
  while (e < text.length && IDENT.test(text[e])) e++;
  return s < e ? [s, e] : null;
}

function enclosingMember(cls: MockClass | undefined, offset: number): MockMember | undefined {
  return cls?.members.find((m) => m.kind === "method" && m.start <= offset && offset <= m.end);
}

/** Possible classes of the expression ending at `end` (before `->` / `::`). */
function receiverTypes(path: string, text: string, file: MockFile, end: number, buffers: Buffers, depth = 0): string[] {
  if (depth > 3) return [];
  let s = end;
  while (s > 0 && IDENT.test(text[s - 1])) s--;
  const recv = text.slice(s, end);
  const cls = classAt(file, end);
  const lower = recv.toLowerCase();
  if (!recv) return [];
  if (lower === "$this" || lower === "self" || lower === "static") return cls ? [cls.fqn] : [];
  if (lower === "parent") return cls?.extends ? [resolveName(cls.extends, file)] : [];
  if (recv.startsWith("$")) {
    const method = enclosingMember(cls, end);
    const scope = method ? text.slice(method.start, end) : text.slice(0, end);
    const v = recv.replace("$", "\\$");
    const param = new RegExp(`([?\\w\\\\|]+)\\s+${v}\\b`).exec(method?.params ?? "");
    if (param) return [resolveName(param[1].replace(/^\?/, "").split("|")[0], file)];
    const caught = new RegExp(`catch\\s*\\(([^)]*?)\\s*${v}\\s*\\)`).exec(scope);
    if (caught) return caught[1].split("|").map((t) => resolveName(t.trim(), file));
    const created = new RegExp(`${v}\\s*=\\s*new\\s+([\\w\\\\]+)`).exec(scope);
    if (created) return [resolveName(created[1], file)];
    return [];
  }
  const before = text.slice(0, s).trimEnd();
  if (before.endsWith("->")) {
    const owners = receiverTypes(path, text, file, before.length - 2, buffers, depth + 1);
    for (const owner of owners) {
      const found = findMember(owner, recv, ["property"], buffers);
      const type = found && memberType(found.member);
      if (type) return [resolveName(type.replace(/^\?/, "").split("|")[0], found.decl.file)];
    }
    return [];
  }
  return [resolveName(recv, file)];
}

function resolveAt(path: string, text: string, offset: number, buffers?: Buffers): Resolution | null {
  const w = wordAt(text, offset);
  if (!w) return null;
  const [start, end] = w;
  const word = text.slice(start, end);
  const file = parse(path, text);
  const cls = classAt(file, start);
  // Declarations.
  for (const c of file.classes) {
    if (c.offset === start) return { start, end, decl: true, sym: { kind: "class", fqn: c.fqn } };
    for (const m of c.members) {
      const at = m.offset;
      if (at === start || (m.kind === "property" && at === start + 1)) {
        return { start, end, decl: true, sym: { kind: "member", cls: c.fqn, name: m.name, member: m.kind } };
      }
    }
  }
  const before = text.slice(0, start).trimEnd();
  const after = text.slice(end).trimStart();
  const op = before.endsWith("->") ? "->" : before.endsWith("::") ? "::" : null;
  if (op) {
    const name = word.replace(/^\$/, "");
    const kinds: MockMember["kind"][] = after.startsWith("(")
      ? ["method"]
      : op === "->" || word.startsWith("$")
        ? ["property"]
        : word === "class"
          ? []
          : ["constant", "enumCase"];
    const types = receiverTypes(path, text, file, before.length - op.length, buffers);
    if (!kinds.length) return types[0] ? { start, end, decl: false, sym: { kind: "class", fqn: types[0] } } : null;
    for (const t of types) {
      const found = findMember(t, name, kinds, buffers);
      if (found) return { start, end, decl: false, sym: { kind: "member", cls: found.decl.cls.fqn, name: found.member.name, member: found.member.kind } };
    }
    return null;
  }
  if (word.startsWith("$")) {
    if (word === "$this") return cls ? { start, end, decl: false, sym: { kind: "class", fqn: cls.fqn } } : null;
    const method = enclosingMember(cls, start);
    const scopeStart = method ? method.start : 0;
    const scopeEnd = method ? method.end : text.length;
    const re = new RegExp(`${word.replace("$", "\\$")}(?![\\w])`, "g");
    re.lastIndex = scopeStart;
    const first = re.exec(text);
    const at = first && first.index < scopeEnd ? first.index : start;
    return { start, end, decl: at === start, sym: { kind: "variable", name: word, path, at } };
  }
  const bare = word.replace(/^\\/, "");
  if (KEYWORDS.has(bare.toLowerCase()) && !word.startsWith("\\")) {
    if (bare === "self" || bare === "static") return cls ? { start, end, decl: false, sym: { kind: "class", fqn: cls.fqn } } : null;
    return null;
  }
  // `use` lines name classes fully qualified.
  const lineStart = text.lastIndexOf("\n", start - 1) + 1;
  if (/^use\s/.test(text.slice(lineStart, start))) return { start, end, decl: false, sym: { kind: "class", fqn: bare } };
  if (after.startsWith("(") && !/\bnew$/.test(before)) return { start, end, decl: false, sym: { kind: "function", name: bare.split("\\").pop()! } };
  const fqn = resolveName(word, file);
  if (findClass(fqn, buffers)) return { start, end, decl: false, sym: { kind: "class", fqn } };
  if (stubConstants().some((k) => k.name === bare)) return { start, end, decl: false, sym: { kind: "constant", name: bare } };
  return { start, end, decl: false, sym: { kind: "class", fqn } };
}

// ---- Targets ----


function target(path: string, text: string, offset: number, name: string, container?: string): NavTarget {
  const p = positionOf(text, offset);
  return { path, line: p.line, column: p.character, name, container: container || null };
}

function stubExtension(path: string): string {
  return path.slice(STUBS.length).split("/")[0];
}

function memberLabel(c: MockClass, m: MockMember): string {
  return m.kind === "method" ? `${c.name}::${m.name}()` : m.kind === "property" ? `${c.name}::$${m.name}` : `${c.name}::${m.name}`;
}

function declTarget(sym: Sym, buffers?: Buffers): NavTarget | null {
  switch (sym.kind) {
    case "class": {
      const d = findClass(sym.fqn, buffers);
      if (!d) return null;
      return target(d.path, d.text, d.cls.offset, d.cls.name, d.path.startsWith(STUBS) ? stubExtension(d.path) : d.cls.namespace);
    }
    case "member": {
      const found = findMember(sym.cls, sym.name, [sym.member], buffers);
      if (!found) return null;
      const { decl, member } = found;
      return target(decl.path, decl.text, member.offset, memberLabel(decl.cls, member), decl.path.startsWith(STUBS) ? stubExtension(decl.path) : decl.cls.namespace);
    }
    case "function": {
      const f = stubFunctions().find((x) => x.name.toLowerCase() === sym.name.toLowerCase());
      return f ? target(f.path, textOf(f.path)!, f.offset, `${f.name}()`, stubExtension(f.path)) : null;
    }
    case "constant": {
      const k = stubConstants().find((x) => x.name === sym.name);
      return k ? target(k.path, textOf(k.path)!, k.offset, k.name, stubExtension(k.path)) : null;
    }
    case "variable": {
      const text = textOf(sym.path, buffers) ?? "";
      return target(sym.path, text, sym.at, sym.name);
    }
  }
}

function signatureOf(c: MockClass): string {
  const mods = [c.modifiers.isFinal && "final", c.modifiers.isAbstract && "abstract"].filter(Boolean).join(" ");
  const ext = c.extends ? ` extends ${c.extends.replace(/^\\/, "").split("\\").pop()}` : "";
  const impl = c.implements.length ? ` implements ${c.implements.map((i) => i.split("\\").pop()).join(", ")}` : "";
  return `${mods ? `${mods} ` : ""}${c.kind} ${c.name}${ext}${impl}`;
}

function infoOf(sym: Sym, buffers?: Buffers): { container: string; signature: string } | null {
  switch (sym.kind) {
    case "class": {
      const d = findClass(sym.fqn, buffers);
      if (!d) return null;
      const container = d.path.startsWith(STUBS) ? `PHP Runtime · ${stubExtension(d.path)}` : d.cls.namespace;
      return { container, signature: signatureOf(d.cls) };
    }
    case "member": {
      const found = findMember(sym.cls, sym.name, [sym.member], buffers);
      if (!found) return null;
      const m = found.member;
      const sig =
        m.kind === "method"
          ? `${m.visibility}${m.modifiers.isStatic ? " static" : ""} function ${m.name}(${m.params ?? ""})${m.type ? `: ${m.type}` : ""}`
          : m.kind === "property"
            ? `${m.visibility} ${m.type ? `${m.type} ` : ""}$${m.name}`
            : `const ${m.name}${m.value ? ` = ${m.value}` : ""}`;
      return { container: found.decl.cls.fqn, signature: sig };
    }
    case "function": {
      const f = stubFunctions().find((x) => x.name.toLowerCase() === sym.name.toLowerCase());
      return f ? { container: `PHP Runtime · ${stubExtension(f.path)}`, signature: f.signature } : null;
    }
    case "constant": {
      const k = stubConstants().find((x) => x.name === sym.name);
      return k ? { container: `PHP Runtime · ${stubExtension(k.path)}`, signature: k.signature } : null;
    }
    case "variable":
      return { container: "", signature: sym.name };
  }
}

function displayOf(sym: Sym): string {
  switch (sym.kind) {
    case "class":
      return sym.fqn.split("\\").pop()!;
    case "member": {
      const c = sym.cls.split("\\").pop();
      return sym.member === "method" ? `${c}::${sym.name}()` : sym.member === "property" ? `${c}::$${sym.name}` : `${c}::${sym.name}`;
    }
    case "function":
      return `${sym.name}()`;
    default:
      return sym.name;
  }
}

// ---- Usages ----

/** Whether two classes are related (one extends or implements the other). */
function related(a: string, b: string, buffers?: Buffers): boolean {
  if (a.toLowerCase() === b.toLowerCase()) return true;
  const da = findClass(a, buffers);
  const db = findClass(b, buffers);
  return (
    (!!da && lineage(da, buffers).some((x) => x.cls.fqn.toLowerCase() === b.toLowerCase())) ||
    (!!db && lineage(db, buffers).some((x) => x.cls.fqn.toLowerCase() === a.toLowerCase()))
  );
}

/** Same symbol; method calls on related classes count, like PHPStorm's base method search. */
function sameSymbol(a: Sym, b: Sym, buffers?: Buffers): boolean {
  if (a.kind !== b.kind) return false;
  switch (a.kind) {
    case "class":
      return a.fqn.toLowerCase() === (b as typeof a).fqn.toLowerCase();
    case "member": {
      const m = b as typeof a;
      return a.member === m.member && a.name.toLowerCase() === m.name.toLowerCase() && related(a.cls, m.cls, buffers);
    }
    case "function":
      return a.name.toLowerCase() === (b as typeof a).name.toLowerCase();
    case "constant":
      return a.name === (b as typeof a).name;
    case "variable": {
      const v = b as typeof a;
      return a.path === v.path && a.at === v.at;
    }
  }
}

function searchWord(sym: Sym): string {
  switch (sym.kind) {
    case "class":
      return sym.fqn.split("\\").pop()!;
    default:
      return sym.name;
  }
}

function usagesOf(sym: Sym, buffers?: Buffers): { declaration: UsageLocation | null; usages: UsageLocation[] } {
  const declTargetOf = declTarget(sym, buffers);
  const declaration: UsageLocation | null = declTargetOf
    ? {
        path: declTargetOf.path,
        range: {
          start: { line: declTargetOf.line, character: declTargetOf.column },
          end: { line: declTargetOf.line, character: declTargetOf.column + searchWord(sym).length },
        },
      }
    : null;
  const word = searchWord(sym);
  const usages: UsageLocation[] = [];
  const files = sym.kind === "variable" ? [{ path: sym.path, text: textOf(sym.path, buffers) ?? "" }] : sources(buffers).filter((s) => !s.stub);
  for (const f of files) {
    const re = new RegExp(`(?<![\\w])${word.replace(/[$\\]/g, (c) => `\\${c}`)}(?![\\w])`, "g");
    for (const m of f.text.matchAll(re)) {
      const r = resolveAt(f.path, f.text, m.index! + 1, buffers);
      // Declarations (also of implementations) are no usages.
      if (!r || (r.decl && sym.kind !== "variable") || !sameSymbol(sym, r.sym, buffers)) continue;
      const start = positionOf(f.text, m.index!);
      const end = positionOf(f.text, m.index! + word.length);
      usages.push({ path: f.path, range: { start, end } });
    }
  }
  return { declaration, usages };
}

async function grouped(usages: UsageLocation[], declaration: UsageLocation | null, buffers?: Buffers) {
  const files: Record<string, string> = {};
  for (const u of usages) files[u.path] ??= textOf(u.path, buffers) ?? "";
  const wasm = await forgeWasm();
  return callWasm<{ byType: UsageTree; byFile: UsageTree }>(wasm.codeintel_group_usages, { usages, declaration, files });
}

// ---- Gutter markers ----

function lineMarkers(path: string, text: string, buffers?: Buffers): LineMarkerInfo[] {
  const file = parse(path, text);
  const all = allClasses(buffers).filter((d) => !d.path.startsWith(STUBS) && d.path !== path);
  all.push(...file.classes.map((cls) => ({ path, text, file, cls })));
  const out: LineMarkerInfo[] = [];
  const names = (ts: NavTarget[], prefix: string, what: string) =>
    ts.length === 1 ? `${prefix} ${ts[0].name.split("::")[0]}` : `${prefix} ${ts.length} ${what}`;
  for (const cls of file.classes) {
    const self: Decl = { path, text, file, cls };
    const below = all.filter((d) => d.cls.fqn !== cls.fqn && lineage(d, buffers).some((a) => a.cls.fqn === cls.fqn));
    const line = positionOf(text, cls.offset).line;
    if (below.length) {
      const targets = below.map((d) => target(d.path, d.text, d.cls.offset, d.cls.name, d.cls.namespace));
      const iface = cls.kind === "interface";
      out.push({
        line,
        kind: iface ? "implemented" : "subclassed",
        title: iface ? names(targets, "Is implemented in", "classes") : names(targets, "Is subclassed by", "classes"),
        targets,
      });
    }
    const ancestors = lineage(self, buffers).slice(1);
    for (const m of cls.members.filter((x) => x.kind === "method")) {
      const mline = positionOf(text, m.offset).line;
      const up: NavTarget[] = [];
      let implementsIt = false;
      for (const a of ancestors) {
        const am = a.cls.members.find((x) => x.kind === "method" && x.name.toLowerCase() === m.name.toLowerCase());
        if (!am) continue;
        if (!up.length) implementsIt = a.cls.kind === "interface" || !!am.modifiers.isAbstract;
        up.push(target(a.path, a.text, am.offset, memberLabel(a.cls, am), a.path.startsWith(STUBS) ? stubExtension(a.path) : a.cls.namespace));
      }
      if (up.length) {
        out.push({
          line: mline,
          kind: implementsIt ? "implements" : "overrides",
          title: `${implementsIt ? "Implements" : "Overrides"} method in ${up[0].name.split("::")[0]}`,
          targets: up,
        });
      }
      const down = below.flatMap((d) => {
        const sm = d.cls.members.find((x) => x.kind === "method" && x.name.toLowerCase() === m.name.toLowerCase());
        return sm ? [target(d.path, d.text, sm.offset, memberLabel(d.cls, sm), d.cls.namespace)] : [];
      });
      if (down.length) {
        const impl = cls.kind === "interface" || !!m.modifiers.isAbstract;
        out.push({
          line: mline,
          kind: impl ? "implemented" : "overridden",
          title: impl ? names(down, "Is implemented in", "classes") : names(down, "Is overridden in", "subclasses"),
          targets: down,
        });
      }
    }
  }
  return out;
}

// ---- Commands ----

interface At {
  path: string;
  line: number;
  character: number;
  content?: string;
  buffers?: Record<string, string>;
}

function at(args: Record<string, unknown>): { a: At; text: string; offset: number; buffers: Buffers } {
  const a = args as unknown as At;
  const buffers: Record<string, string> = { ...(a.buffers ?? {}) };
  if (typeof a.content === "string") buffers[a.path] = a.content;
  const text = textOf(a.path, buffers) ?? "";
  return { a, text, offset: offsetOf(text, { line: a.line, character: a.character }), buffers };
}

/** Other languages: the language server mocks, like the backend asks the servers first. */
async function serverDefinition(a: At): Promise<DeclarationAnswer> {
  const locs = await invoke<EditorLocation[]>("lsp_definition", { path: a.path, line: a.line, character: a.character }).catch(() => []);
  const here = (l: EditorLocation) =>
    l.path === a.path &&
    (l.range.start.line < a.line || (l.range.start.line === a.line && l.range.start.character <= a.character)) &&
    (l.range.end.line > a.line || (l.range.end.line === a.line && a.character <= l.range.end.character));
  if (locs.some(here)) return { onDeclaration: true, targets: [], source: "lsp" };
  if (!locs.length) return { onDeclaration: false, targets: [], source: "none", message: "Cannot find declaration to go to" };
  return {
    onDeclaration: false,
    targets: locs.map((l) => ({ path: l.path, line: l.range.start.line, column: l.range.start.character, name: "" })),
    source: "lsp",
  };
}

registerMock("code_goto_declaration", async (args): Promise<DeclarationAnswer> => {
  const { a, text, offset, buffers } = at(args);
  if (!isPhp(a.path)) return serverDefinition(a);
  const r = resolveAt(a.path, text, offset, buffers);
  if (!r) return { onDeclaration: false, targets: [], source: "none", message: "Cannot find declaration to go to" };
  if (r.decl) return { onDeclaration: true, title: displayOf(r.sym), targets: [], source: "index" };
  const t = declTarget(r.sym, buffers);
  if (!t) return { onDeclaration: false, title: displayOf(r.sym), targets: [], source: "none", message: "Cannot find declaration to go to" };
  return { onDeclaration: false, title: displayOf(r.sym), targets: [t], source: t.path.startsWith(STUBS) ? "stubs" : "index" };
});

registerMock("code_symbol_info", (args): SymbolHover | null => {
  const { a, text, offset, buffers } = at(args);
  if (!isPhp(a.path)) return null;
  const r = resolveAt(a.path, text, offset, buffers);
  if (!r || !declTarget(r.sym, buffers)) return null;
  const info = infoOf(r.sym, buffers);
  return { range: { start: positionOf(text, r.start), end: positionOf(text, r.end) }, container: info?.container ?? "", signature: info?.signature ?? "" };
});

registerMock("code_find_usages", async (args): Promise<UsagesAnswer> => {
  const { a, text, offset, buffers } = at(args);
  if (!isPhp(a.path)) {
    const refs = await invoke<EditorLocation[]>("lsp_references", { path: a.path, line: a.line, character: a.character }).catch(() => []);
    const w = wordAt(text, offset);
    const trees = await grouped(refs.map((l) => ({ path: l.path, range: l.range })), null, buffers);
    return { title: w ? text.slice(...w) : "", source: refs.length ? "lsp" : "none", declaration: null, ...trees };
  }
  const r = resolveAt(a.path, text, offset, buffers);
  if (!r) {
    const empty = { total: 0, fileCount: 0, groups: [] };
    return { title: wordAt(text, offset) ? text.slice(...wordAt(text, offset)!) : "", source: "none", declaration: null, byType: empty, byFile: empty };
  }
  const { declaration, usages } = usagesOf(r.sym, buffers);
  const trees = await grouped(usages, declaration, buffers);
  return { title: displayOf(r.sym), source: "index", declaration, ...trees };
});

registerMock("code_navigate", (args): NavigateAnswer => {
  const { a, text, offset, buffers } = at(args);
  const kind = (args as { kind: NavKind }).kind;
  if (!isPhp(a.path)) return { targets: [] };
  const file = parse(a.path, text);
  const r = resolveAt(a.path, text, offset, buffers);
  const classes = allClasses(buffers).filter((d) => !d.path.startsWith(STUBS));
  if (kind === "implementation") {
    if (r?.sym.kind === "class") {
      const fqn = r.sym.fqn;
      return {
        targets: classes
          .filter((d) => d.cls.fqn !== fqn && lineage(d, buffers).some((x) => x.cls.fqn === fqn))
          .map((d) => target(d.path, d.text, d.cls.offset, d.cls.name, d.cls.namespace)),
      };
    }
    if (r?.sym.kind === "member") {
      const { cls, name } = r.sym;
      return {
        targets: classes.flatMap((d) => {
          if (d.cls.fqn === cls || !lineage(d, buffers).some((x) => x.cls.fqn === cls)) return [];
          const m = d.cls.members.find((x) => x.name === name);
          return m ? [target(d.path, d.text, m.offset, memberLabel(d.cls, m), d.cls.namespace)] : [];
        }),
      };
    }
    return { targets: [] };
  }
  if (kind === "superMethod") {
    const cls = classAt(file, offset);
    if (!cls) return { targets: [] };
    const method = enclosingMember(cls, offset) ?? (r?.sym.kind === "member" ? cls.members.find((m) => m.name === (r.sym as { name: string }).name) : undefined);
    const self: Decl = { path: a.path, text, file, cls };
    const ancestors = lineage(self, buffers).slice(1);
    if (!method) return { targets: ancestors.map((d) => target(d.path, d.text, d.cls.offset, d.cls.name, d.cls.namespace)) };
    return {
      targets: ancestors.flatMap((d) => {
        const m = d.cls.members.find((x) => x.kind === "method" && x.name.toLowerCase() === method.name.toLowerCase());
        return m ? [target(d.path, d.text, m.offset, memberLabel(d.cls, m), d.cls.namespace)] : [];
      }),
    };
  }
  // Type declaration.
  if (r?.sym.kind === "variable") {
    const types = receiverTypes(a.path, text, file, r.end, buffers);
    return { targets: types.flatMap((t) => declTarget({ kind: "class", fqn: t }, buffers) ?? []) };
  }
  if (r?.sym.kind === "member") {
    const found = findMember(r.sym.cls, r.sym.name, [r.sym.member], buffers);
    const type = found && memberType(found.member);
    const t = type && declTarget({ kind: "class", fqn: resolveName(type.replace(/^\?/, ""), found.decl.file) }, buffers);
    return { targets: t ? [t] : [] };
  }
  const t = r?.sym.kind === "class" ? declTarget(r.sym, buffers) : null;
  return { targets: t ? [t] : [] };
});

registerMock("code_line_markers", (args): LineMarkerInfo[] => {
  const { path, content } = args as { path: string; content?: string };
  const text = content ?? textOf(path);
  return text !== undefined && isPhp(path) ? lineMarkers(path, text) : [];
});

registerMock("library_read", ({ path }): LibraryFile => {
  const p = String(path);
  const text = p.startsWith(STUBS) ? demoStubs[p.slice(STUBS.length)] : undefined;
  if (text === undefined) throw new Error(`not a library file: ${p}`);
  return { text, library: "PHP Runtime", detail: demoStubsDetail };
});

// Rename and context actions: word-based in browser mode, edits applied by forge-codeintel.

registerMock("code_prepare_rename", (args) => {
  const { a, text, offset, buffers } = at(args);
  const r = isPhp(a.path) ? resolveAt(a.path, text, offset, buffers) : null;
  if (!r) return { invalid: true };
  return { range: { start: positionOf(text, r.start), end: positionOf(text, r.end) }, placeholder: text.slice(r.start, r.end), invalid: false };
});

registerMock("code_rename", async (args) => {
  const { a, text, offset, buffers } = at(args);
  const newName = String((args as { newName: string }).newName);
  const r = resolveAt(a.path, text, offset, buffers);
  if (!r) throw new Error("Nothing to rename here");
  const { declaration, usages } = usagesOf(r.sym, buffers);
  const all = declaration ? [declaration, ...usages] : usages;
  const byPath = new Map<string, { range: UsageLocation["range"]; newText: string }[]>();
  for (const u of all) {
    if (u.path.startsWith(STUBS)) continue;
    const list = byPath.get(u.path) ?? [];
    if (!list.some((e) => e.range.start.line === u.range.start.line && e.range.start.character === u.range.start.character)) {
      list.push({ range: u.range, newText: newName.replace(/^\$/, r.sym.kind === "variable" ? "$" : "") });
    }
    byPath.set(u.path, list);
  }
  const edit: WorkspaceEdit = { changes: [...byPath].map(([path, edits]) => ({ kind: "edit" as const, path, edits })) };
  const files: Record<string, string> = {};
  for (const path of byPath.keys()) files[path] = textOf(path, buffers) ?? "";
  const wasm = await forgeWasm();
  const preview = await callWasm(wasm.codeintel_preview_edit, { edit, files });
  return { edit, preview };
});

registerMock("code_apply_edit", async ({ edit, skip }): Promise<AppliedFile[]> => {
  const e = edit as WorkspaceEdit;
  const skipped = new Set((skip as string[] | undefined) ?? []);
  const changes = e.changes.filter((c) => !(c.kind === "edit" && skipped.has(c.path)));
  const files: Record<string, string> = {};
  for (const c of changes) {
    const p = c.kind === "rename" ? c.from : c.path;
    if (mockFiles[p] !== undefined) files[p] = mockFiles[p];
  }
  const wasm = await forgeWasm();
  const out = await callWasm<{ applied: AppliedFile[]; files: Record<string, string> }>(wasm.codeintel_apply_edit, {
    edit: { changes },
    files,
  });
  for (const f of out.applied) {
    if (f.after === null) delete mockFiles[f.path];
    else mockFiles[f.path] = f.after;
  }
  return out.applied;
});

registerMock("code_revert_edit", ({ files }) => {
  for (const f of files as AppliedFile[]) {
    if (f.before === null) delete mockFiles[f.path];
    else mockFiles[f.path] = f.before;
  }
});

registerMock("code_context_actions", async (): Promise<ContextAction[]> => []);
registerMock("code_resolve_action", ({ raw }) => raw as ContextAction);
registerMock("code_execute_command", () => null);
