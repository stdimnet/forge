/**
 * Browser-mode stand-in for Create from usage (`phpforge_lang_php::create`): "Create method
 * 'foo'" for `$this->foo(...)` and "Create class 'Foo'" for `new Foo(...)` in the demo
 * project, enough to show the quick fixes and the Create New PHP Class dialog. The real
 * analysis (fields, constants, functions, other files, inferred types) runs in Rust.
 * Nothing here writes files: the edits go through the in-memory workspace mock.
 */
import { registerMock } from "@/ipc";
import type { ContextAction, CreateClassData } from "@/ipc/codeintel";
import type { LspPosition } from "@/ipc/lsp";
import { classAt, parsePhp } from "./phpMockParser";
import { mockFiles } from "./workspaceMock";

function position(text: string, offset: number): LspPosition {
  const before = text.slice(0, offset);
  const line = before.split("\n").length - 1;
  return { line, character: offset - (before.lastIndexOf("\n") + 1) };
}

function wordAt(text: string, offset: number): [number, number] | null {
  const ident = /[\w$\\]/;
  let s = offset;
  while (s > 0 && ident.test(text[s - 1])) s--;
  let e = offset;
  while (e < text.length && ident.test(text[e])) e++;
  return s < e ? [s, e] : null;
}

function matchingParen(text: string, open: number): number {
  let depth = 0;
  for (let i = open; i < text.length; i++) {
    if (text[i] === "(") depth++;
    else if (text[i] === ")" && --depth === 0) return i;
  }
  return -1;
}

const base = { source: "php", isPreferred: false, needsResolve: false } as const;

function createMethod(path: string, text: string, start: number, end: number): ContextAction[] {
  const name = text.slice(start, end);
  const cls = classAt(parsePhp(text), start);
  if (!cls || cls.members.some((m) => m.kind === "method" && m.name.toLowerCase() === name.toLowerCase())) return [];
  const open = text.indexOf("(", end);
  const close = matchingParen(text, open);
  if (close < 0) return [];
  const enclosing = cls.members.find((m) => m.kind === "method" && m.start <= start && start <= m.end);
  const signature = enclosing ? text.slice(enclosing.offset, text.indexOf("{", enclosing.offset)) : "";
  const used = new Set<string>();
  const params = text
    .slice(open + 1, close)
    .split(",")
    .map((a) => a.trim())
    .filter(Boolean)
    .map((arg) => {
      const variable = /^\$(\w+)$/.exec(arg)?.[1];
      let [type, param] = variable
        ? [new RegExp(`([?\\w\\\\]+)\\s+\\$${variable}\\b`).exec(signature)?.[1], variable]
        : /^['"]/.test(arg)
          ? ["string", "string"]
          : /^-?\d+$/.test(arg)
            ? ["int", "int"]
            : /^-?\d*\.\d+$/.test(arg)
              ? ["float", "float"]
              : [undefined, "param"];
      for (let n = 1; used.has(param); n++) param = `${param.replace(/\d+$/, "")}${n}`;
      used.add(param);
      return `${type ? `${type} ` : ""}$${param}`;
    });
  const statement = text.slice(close + 1).trimStart().startsWith(";") && /[;{}]$/.test(text.slice(0, start - "$this->".length).trimEnd());
  const at = enclosing ? enclosing.end : text.slice(0, cls.close).trimEnd().length;
  const block = `\n\n    private function ${name}(${params.join(", ")})${statement ? ": void" : ""}\n    {\n        // TODO: Implement ${name}() method.\n    }`;
  const after = text.slice(0, at) + block + text.slice(at);
  const caret = position(after, at + block.indexOf("// TODO"));
  return [
    {
      ...base,
      title: `Create method '${name}'`,
      kind: "quickFix",
      edit: { changes: [{ kind: "edit", path, edits: [{ range: { start: position(text, at), end: position(text, at) }, newText: block }] }] },
      caret: { path, range: { start: caret, end: caret } },
    },
  ];
}

function createClass(path: string, text: string, name: string, start: number): ContextAction[] {
  const file = parsePhp(text);
  const declared = new RegExp(`\\b(class|interface|trait|enum)\\s+${name}\\b`);
  if (Object.values(mockFiles).some((t) => declared.test(t))) return [];
  const fqn = file.uses.get(name) ?? (file.namespace ? `${file.namespace}\\${name}` : name);
  const data: CreateClassData = {
    name,
    namespace: fqn.includes("\\") ? fqn.slice(0, fqn.lastIndexOf("\\")) : "",
    template: "PHP Class",
    extends: /throw\s+new\s*$/.test(text.slice(0, start)) ? "\\Exception" : null,
    import: !file.uses.has(name),
    path,
  };
  return [{ ...base, title: `Create class '${name}'`, kind: "quickFix", action: "PhpCreateClass", data }];
}

/** The Create ... quick fixes at `offset` of the demo file `path`. */
export function createFromUsage(path: string, text: string, offset: number): ContextAction[] {
  const word = wordAt(text, offset);
  if (!word) return [];
  const [start, end] = word;
  const name = text.slice(start, end);
  const before = text.slice(0, start).trimEnd();
  const after = text.slice(end).trimStart();
  if (before.endsWith("$this->") && after.startsWith("(") && /^\w+$/.test(name)) return createMethod(path, text, start, end);
  if (/\bnew$/.test(before) && /^[A-Z]\w*$/.test(name)) return createClass(path, text, name, start);
  return [];
}

registerMock("php_import_class", ({ content, fqn }) => {
  const text = String(content);
  const name = String(fqn).replace(/^\\+/, "");
  const ns = name.includes("\\") ? name.slice(0, name.lastIndexOf("\\")) : "";
  const file = parsePhp(text);
  if (file.namespace.toLowerCase() === ns.toLowerCase() || file.uses.has(name.split("\\").pop()!)) return [];
  const uses = [...text.matchAll(/^use [^;]+;$/gm)];
  const last = uses[uses.length - 1];
  const at = last ? last.index! + last[0].length : text.indexOf(";", text.indexOf("namespace ")) + 1;
  return [{ start: at, end: at, newText: last ? `\nuse ${name};` : `\n\nuse ${name};` }];
});
