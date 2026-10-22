/**
 * Browser-mode Type, Method and Call Hierarchy over the demo project (the desktop app answers
 * from the language servers and the tree-sitter PHP index, see
 * app/src-tauri/src/commands/hierarchy.rs). The demo's classes come from the small regex
 * parser of the PHP mocks and become the same data the commands answer; the views are
 * computed by Rust (`forge_codeintel::hierarchy`) as WebAssembly, like in the desktop app.
 */
import { registerMock } from "@/ipc";
import type {
  CallDirection,
  CallHierarchyAnswer,
  CallNode,
  CallSite,
  HierarchyItem,
  HierarchyKind,
  TypeEntry,
  TypeGraph,
  TypeHierarchyAnswer,
} from "@/ipc/hierarchy";
import type { LspPosition } from "@/ipc/lsp";
import { offsetOf, positionOf } from "./codeintelMock";
import { classAt, parsePhp, type MockClass, type MockFile, type MockMember } from "./phpMockParser";
import { mockFiles } from "./workspaceMock";

const isPhp = (p: string) => /\.(php|phtml|inc)$/i.test(p) && !p.endsWith(".blade.php");
const isVendor = (p: string) => p.startsWith("vendor/") || p.includes("/vendor/");

interface Args {
  path: string;
  line: number;
  character: number;
  content?: string;
}

interface Decl {
  path: string;
  text: string;
  file: MockFile;
  cls: MockClass;
}

function files(current?: { path: string; text: string }): { path: string; text: string; file: MockFile }[] {
  return Object.keys(mockFiles)
    .filter((p) => isPhp(p) && !isVendor(p))
    .sort()
    .map((path) => {
      const text = path === current?.path ? current.text : mockFiles[path];
      return { path, text, file: parsePhp(text) };
    });
}

function declarations(current?: { path: string; text: string }): Decl[] {
  return files(current).flatMap((f) => f.file.classes.map((cls) => ({ ...f, cls })));
}

function resolve(name: string, file: MockFile): string {
  if (name.startsWith("\\")) return name.slice(1);
  const [head, ...rest] = name.split("\\");
  const imported = file.uses.get(head);
  if (imported) return [imported, ...rest].join("\\");
  return file.namespace ? `${file.namespace}\\${name}` : name;
}

function at(text: string, offset: number): LspPosition {
  return positionOf(text, offset);
}

function typeKind(kind: string): HierarchyKind {
  return kind === "interface" || kind === "trait" || kind === "enum" ? kind : "class";
}

function typeItem(d: Decl): HierarchyItem {
  const p = at(d.text, d.cls.offset);
  return {
    id: d.cls.fqn,
    name: d.cls.name,
    kind: typeKind(d.cls.kind),
    container: d.cls.namespace || null,
    path: d.path,
    line: p.line,
    column: p.character,
    isAbstract: !!d.cls.modifiers.isAbstract,
  };
}

/** Every project class with its supertypes; types only known by name become entries too. */
function graphOf(decls: Decl[], root: string, method?: string): TypeGraph {
  const types: TypeEntry[] = decls.map((d) => {
    const m = method ? d.cls.members.find((x) => x.kind === "method" && x.name.toLowerCase() === method.toLowerCase()) : undefined;
    const presence = m ? (m.modifiers.isAbstract || d.cls.kind === "interface" ? "abstract" : "defined") : null;
    const parents = d.cls.extends ? [resolve(d.cls.extends, d.file)] : [];
    const ifaces = d.cls.implements.map((i) => resolve(i, d.file));
    return {
      item: typeItem(d),
      extends: d.cls.kind === "interface" ? [...parents, ...ifaces] : parents,
      implements: d.cls.kind === "interface" ? [] : ifaces,
      method: presence,
    };
  });
  const known = new Set(types.map((t) => t.item.id));
  for (const t of [...types]) {
    for (const [ids, kind] of [
      [t.extends ?? [], t.item.kind === "interface" ? "interface" : "class"],
      [t.implements ?? [], "interface"],
    ] as const) {
      for (const id of ids) {
        if (known.has(id)) continue;
        known.add(id);
        const name = id.split("\\").pop()!;
        types.push({ item: { id, name, kind, container: id.slice(0, -name.length - 1) || null, path: "", line: 0, column: 0 } });
      }
    }
  }
  return { root, types, method: method ?? null };
}

function wordAt(text: string, offset: number): string {
  const before = /[\w\\]*$/.exec(text.slice(0, offset))?.[0] ?? "";
  const after = /^\w*/.exec(text.slice(offset))?.[0] ?? "";
  return before + after;
}

function source(a: Args) {
  const text = a.content ?? mockFiles[a.path] ?? "";
  const offset = offsetOf(text, { line: a.line, character: a.character });
  const file = parsePhp(text);
  return { text, offset, file };
}

registerMock("code_type_hierarchy", async (args): Promise<TypeHierarchyAnswer> => {
  const a = args as unknown as Args;
  if (!isPhp(a.path)) return { title: "", graph: null, source: "none", message: "No class, interface, trait or enum at the caret" };
  const { text, offset, file } = source(a);
  const decls = declarations({ path: a.path, text });
  const word = wordAt(text, offset);
  const named = word ? decls.find((d) => d.cls.fqn.toLowerCase() === resolve(word, file).toLowerCase()) : undefined;
  const cls = named?.cls ?? classAt(file, offset);
  if (!cls) return { title: "", graph: null, source: "none", message: "No class, interface, trait or enum at the caret" };
  return { title: cls.name, graph: graphOf(decls, cls.fqn), source: "index" };
});

function methodAt(file: MockFile, text: string, offset: number): { cls: MockClass; method: MockMember } | undefined {
  const cls = classAt(file, offset);
  if (!cls) return undefined;
  const word = wordAt(text, offset);
  const named = cls.members.find((m) => m.kind === "method" && m.name === word);
  const around = cls.members.find((m) => m.kind === "method" && m.start <= offset && offset <= m.end);
  const method = named ?? around;
  return method ? { cls, method } : undefined;
}

registerMock("code_method_hierarchy", async (args): Promise<TypeHierarchyAnswer> => {
  const a = args as unknown as Args;
  if (!isPhp(a.path)) return { title: "", graph: null, source: "none", message: "Method Hierarchy is available for PHP files" };
  const { text, offset, file } = source(a);
  const found = methodAt(file, text, offset);
  if (!found) return { title: "", graph: null, source: "none", message: "No method at the caret" };
  const decls = declarations({ path: a.path, text });
  return {
    title: `${found.cls.name}::${found.method.name}()`,
    graph: graphOf(decls, found.cls.fqn, found.method.name),
    source: "index",
  };
});

function methodItem(path: string, text: string, cls: MockClass, m: MockMember): HierarchyItem {
  const p = at(text, m.offset);
  return {
    id: `${cls.fqn}::${m.name}()`,
    name: `${m.name}()`,
    kind: m.name === "__construct" ? "constructor" : "method",
    container: cls.fqn,
    path,
    line: p.line,
    column: p.character,
    isAbstract: !!m.modifiers.isAbstract || cls.kind === "interface",
  };
}

registerMock("code_call_hierarchy", async (args): Promise<CallHierarchyAnswer> => {
  const a = args as unknown as Args;
  const none: CallHierarchyAnswer = { title: "", root: null, source: "none", message: "No method or function at the caret" };
  if (!isPhp(a.path)) return none;
  const { text, offset, file } = source(a);
  const found = methodAt(file, text, offset);
  if (!found) return none;
  return {
    title: `${found.cls.name}::${found.method.name}()`,
    root: methodItem(a.path, text, found.cls, found.method),
    source: "index",
  };
});

function lineText(text: string, offset: number): string {
  const start = text.lastIndexOf("\n", offset - 1) + 1;
  const end = text.indexOf("\n", offset);
  return text.slice(start, end < 0 ? text.length : end).trim();
}

function site(path: string, text: string, from: number, to: number): CallSite {
  return { path, range: { start: at(text, from), end: at(text, to) }, preview: lineText(text, from) };
}

function group(found: { item: HierarchyItem; site: CallSite }[]): CallNode[] {
  const out: CallNode[] = [];
  for (const f of found) {
    const node = out.find((n) => n.item.id === f.item.id);
    if (node) node.calls.push(f.site);
    else out.push({ item: f.item, calls: [f.site] });
  }
  return out;
}

/** Calls of `name()` anywhere in the project, by the method or file making them. */
function callers(name: string): CallNode[] {
  const found: { item: HierarchyItem; site: CallSite }[] = [];
  const re = new RegExp(`(?:->|::)\\s*(${name})\\s*\\(`, "g");
  for (const f of files()) {
    for (const m of f.text.matchAll(re)) {
      const from = m.index! + m[0].indexOf(m[1]);
      const cls = classAt(f.file, from);
      const member = cls?.members.find((x) => x.kind === "method" && x.start <= from && from <= x.end);
      const item: HierarchyItem =
        cls && member
          ? methodItem(f.path, f.text, cls, member)
          : { id: f.path, name: f.path.split("/").pop()!, kind: "file", path: f.path, line: 0, column: 0 };
      found.push({ item, site: site(f.path, f.text, from, from + name.length) });
    }
  }
  return group(found);
}

/** Methods called inside the body of the method at `item`. */
function callees(item: HierarchyItem): CallNode[] {
  const text = mockFiles[item.path];
  if (!text) return [];
  const file = parsePhp(text);
  const offset = offsetOf(text, { line: item.line, character: item.column });
  const cls = classAt(file, offset);
  const method = cls?.members.find((m) => m.kind === "method" && m.start <= offset && offset <= m.end);
  if (!cls || !method) return [];
  const decls = declarations();
  const found: { item: HierarchyItem; site: CallSite }[] = [];
  const body = text.slice(method.offset, method.end);
  for (const m of body.matchAll(/(->|::|new\s+)\s*(\w+)\s*\(/g)) {
    const name = m[2];
    const from = method.offset + m.index! + m[0].indexOf(name, m[1].length);
    const isNew = m[1].startsWith("new");
    const target = isNew ? "__construct" : name;
    const own = cls.members.find((x) => x.kind === "method" && x.name === target);
    const decl = isNew
      ? decls.find((d) => d.cls.name === name)
      : own
        ? { path: item.path, text, file, cls }
        : decls.find((d) => d.cls.members.some((x) => x.kind === "method" && x.name === target));
    const member = decl?.cls.members.find((x) => x.kind === "method" && x.name === target);
    const callee: HierarchyItem =
      decl && member
        ? methodItem(decl.path, decl.text, decl.cls, member)
        : { id: `${name}()`, name: `${isNew ? "__construct" : name}()`, kind: "method", container: isNew ? name : null, path: "", line: 0, column: 0 };
    found.push({ item: callee, site: site(item.path, text, from, from + name.length) });
  }
  return group(found);
}

registerMock("code_call_hierarchy_calls", async (args): Promise<CallNode[]> => {
  const { item, direction } = args as { item: HierarchyItem; direction: CallDirection };
  if (!item.path || item.kind === "file") return [];
  const name = item.name.replace(/\(\)$/, "");
  return direction === "callers" ? callers(name) : callees(item);
});
