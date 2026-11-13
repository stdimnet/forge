/**
 * Tiny regex-based PHP analyzer backing the browser-mode PHP mocks. It understands the
 * demo project well enough to drive the Structure view, hover and the Generate actions;
 * the real implementation is the tree-sitter index in packs/phpforge/lang-php.
 */
import type { Modifiers, SymbolKind, Visibility } from "@/features/php/api";

export interface MockMember {
  kind: SymbolKind;
  name: string;
  /** Offset of the name. */
  offset: number;
  /** Start of the declaration (including docblock-free attributes) and end. */
  start: number;
  end: number;
  visibility: Visibility;
  modifiers: Modifiers;
  type?: string;
  params?: string;
  value?: string;
  doc?: string;
  attributes: string[];
  promoted?: boolean;
}

export interface MockClass {
  kind: SymbolKind;
  name: string;
  fqn: string;
  namespace: string;
  offset: number;
  start: number;
  /** Offsets of `{` and `}`. */
  open: number;
  close: number;
  extends?: string;
  implements: string[];
  modifiers: Modifiers;
  doc?: string;
  attributes: string[];
  members: MockMember[];
}

export interface MockFile {
  namespace: string;
  uses: Map<string, string>;
  classes: MockClass[];
}

/** Brace depth at every offset; -1 inside strings and comments. */
function depths(src: string): Int16Array {
  const d = new Int16Array(src.length + 1);
  let depth = 0;
  let i = 0;
  while (i < src.length) {
    const c = src[i];
    const next = src[i + 1];
    if (c === "/" && next === "*") {
      const end = src.indexOf("*/", i + 2);
      const stop = end < 0 ? src.length : end + 2;
      for (; i < stop; i++) d[i] = -1;
      continue;
    }
    if ((c === "/" && next === "/") || (c === "#" && next !== "[")) {
      const end = src.indexOf("\n", i);
      const stop = end < 0 ? src.length : end;
      for (; i < stop; i++) d[i] = -1;
      continue;
    }
    if (c === "'" || c === '"') {
      let j = i + 1;
      while (j < src.length && src[j] !== c) j += src[j] === "\\" ? 2 : 1;
      for (; i <= j && i < src.length; i++) d[i] = -1;
      continue;
    }
    if (c === "{") depth++;
    d[i] = depth;
    if (c === "}") depth--;
    i++;
  }
  d[src.length] = depth;
  return d;
}

function matchingBrace(src: string, open: number, dep: Int16Array): number {
  const level = dep[open];
  for (let i = open + 1; i < src.length; i++) {
    if (src[i] === "}" && dep[i] === level) return i;
  }
  return src.length - 1;
}

/** Docblock and attributes directly above `start`; returns the extended start. */
function leading(src: string, start: number): { doc?: string; attributes: string[]; start: number } {
  const attributes: string[] = [];
  let pos = start;
  let doc: string | undefined;
  for (;;) {
    const before = src.slice(0, pos).replace(/\s+$/, "");
    if (before.endsWith("]")) {
      const open = before.lastIndexOf("#[");
      if (open < 0) break;
      attributes.unshift(before.slice(open + 2, -1).trim());
      pos = open;
      continue;
    }
    if (before.endsWith("*/")) {
      const open = before.lastIndexOf("/**");
      if (open >= 0) doc = before.slice(open);
    }
    break;
  }
  return { doc, attributes, start: pos };
}

const MEMBER_RE =
  /((?:(?:public|protected|private|static|abstract|final|readonly|var)\s+)*)(?:function\s+&?(\w+)\s*\(([^)]*)\)\s*(?::\s*([?\w\\|]+))?|const\s+(?:[\w?]+\s+)?(\w+)\s*=\s*([^;]+);|case\s+(\w+)(?:\s*=\s*([^;]+))?;|(?:([?\w\\|]+)\s+)?\$(\w+)\s*(?:=\s*([^;]+))?;)/g;

function modifiersOf(words: string): { visibility: Visibility; modifiers: Modifiers } {
  const w = words.split(/\s+/);
  const visibility = (w.find((x) => x === "public" || x === "protected" || x === "private") ?? "public") as Visibility;
  return {
    visibility,
    modifiers: {
      isStatic: w.includes("static") || undefined,
      isAbstract: w.includes("abstract") || undefined,
      isFinal: w.includes("final") || undefined,
      isReadonly: w.includes("readonly") || undefined,
    },
  };
}

export function parsePhp(src: string): MockFile {
  const dep = depths(src);
  const namespace = /^\s*namespace\s+([\w\\]+)/m.exec(src)?.[1] ?? "";
  const uses = new Map<string, string>();
  for (const m of src.matchAll(/^use\s+([\w\\]+)(?:\s+as\s+(\w+))?;/gm)) {
    uses.set(m[2] ?? m[1].split("\\").pop()!, m[1]);
  }
  const classes: MockClass[] = [];
  const classRe =
    /((?:(?:abstract|final|readonly)\s+)*)(class|interface|trait|enum)\s+(\w+)(?:\s*:\s*\w+)?(?:\s+extends\s+([\w\\]+))?(?:\s+implements\s+([\w\\,\s]+?))?\s*\{/g;
  for (const m of src.matchAll(classRe)) {
    const at = m.index!;
    if (dep[at] < 0 || src.slice(0, at).trimEnd().endsWith("new")) continue;
    const open = at + m[0].length - 1;
    const close = matchingBrace(src, open, dep);
    const lead = leading(src, at);
    const name = m[3];
    const cls: MockClass = {
      kind: m[2] as SymbolKind,
      name,
      fqn: namespace ? `${namespace}\\${name}` : name,
      namespace,
      offset: at + m[0].indexOf(name, m[1].length + m[2].length),
      start: lead.start,
      open,
      close,
      extends: m[4],
      implements: m[5] ? m[5].split(",").map((s) => s.trim()) : [],
      modifiers: modifiersOf(m[1]).modifiers,
      doc: lead.doc,
      attributes: lead.attributes,
      members: [],
    };
    const body = src.slice(open + 1, close);
    const level = dep[open];
    for (const mm of body.matchAll(MEMBER_RE)) {
      const start = open + 1 + mm.index!;
      // Only direct members (inside the class braces, outside method bodies).
      if (dep[start] !== level) continue;
      // Line comments above a member (`// region Name`) do not hide it.
      let before = src.slice(0, start).trimEnd();
      for (let nl = before.lastIndexOf("\n"); /^\s*(\/\/|#(?!\[))/.test(before.slice(nl + 1)); nl = before.lastIndexOf("\n")) {
        before = before.slice(0, Math.max(0, nl)).trimEnd();
        if (nl < 0) break;
      }
      if (!/[;{}\]/]$/.test(before) && !before.endsWith("*/")) continue;
      const { visibility, modifiers } = modifiersOf(mm[1]);
      const l = leading(src, start);
      const base = { start: l.start, end: start + mm[0].length, visibility, modifiers, doc: l.doc, attributes: l.attributes };
      if (mm[2]) {
        const member: MockMember = {
          ...base,
          kind: "method",
          name: mm[2],
          offset: start + mm[0].indexOf(mm[2], mm[1].length),
          params: mm[3].replace(/\s+/g, " ").trim().replace(/,$/, ""),
          type: mm[4],
        };
        // Body end: the matching brace of the method body.
        const bodyOpen = src.indexOf("{", member.end);
        const semi = src.indexOf(";", member.end);
        if (bodyOpen >= 0 && (semi < 0 || bodyOpen < semi)) member.end = matchingBrace(src, bodyOpen, dep) + 1;
        if (mm[2] === "__construct") {
          for (const p of mm[3].matchAll(/(public|protected|private)\s+(readonly\s+)?([?\w\\|]+)\s+\$(\w+)(?:\s*=\s*([^,)]+))?/g)) {
            cls.members.push({
              ...base,
              kind: "property",
              name: p[4],
              offset: start + mm[0].indexOf(`$${p[4]}`) + 1,
              visibility: p[1] as Visibility,
              modifiers: { isReadonly: p[2] ? true : undefined },
              type: p[3],
              value: p[5]?.trim(),
              doc: undefined,
              attributes: [],
              promoted: true,
            });
          }
        }
        cls.members.push(member);
      } else if (mm[5]) {
        cls.members.push({ ...base, kind: "constant", name: mm[5], offset: start + mm[0].indexOf(mm[5]), value: mm[6].trim() });
      } else if (mm[7]) {
        cls.members.push({ ...base, kind: "enumCase", name: mm[7], offset: start + mm[0].indexOf(mm[7]), value: mm[8]?.trim() });
      } else if (mm[10] && mm[1].trim()) {
        cls.members.push({
          ...base,
          kind: "property",
          name: mm[10],
          offset: start + mm[0].indexOf(`$${mm[10]}`) + 1,
          type: mm[9],
          value: mm[11]?.trim(),
        });
      }
    }
    classes.push(cls);
  }
  return { namespace, uses, classes };
}

export function classAt(file: MockFile, offset: number): MockClass | undefined {
  return file.classes.find((c) => c.start <= offset && offset <= c.close);
}

/** `@var` type of a docblock. */
export function docVarType(doc?: string): string | undefined {
  return doc ? /@var\s+(\S+)/.exec(doc)?.[1] : undefined;
}

export function memberType(m: MockMember): string | undefined {
  return m.type ?? docVarType(m.doc);
}

export function lineOf(src: string, offset: number): number {
  let n = 0;
  for (let i = 0; i < offset && i < src.length; i++) if (src.charCodeAt(i) === 10) n++;
  return n;
}
