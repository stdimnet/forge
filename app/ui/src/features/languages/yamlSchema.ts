/**
 * A small schema engine for YAML files whose keys are known (Docker Compose, Symfony
 * services). It works on the text, so it keeps working while the document does not parse:
 * the key path at the cursor is derived from the indentation of the lines above it.
 */
import type { EditorState, Extension } from "@codemirror/state";
import { syntaxTree } from "@codemirror/language";
import type { Completion, CompletionContext, CompletionResult } from "@codemirror/autocomplete";
import { docDom, hoverDocs, type HoverHit } from "./shared";

export interface SchemaNode {
  /** Markdown documentation shown on hover and next to the completion. */
  doc?: string;
  /** Short type shown as completion detail, e.g. `string`, `list`, `map`. */
  type?: string;
  /** Known values of a scalar. */
  values?: readonly string[];
  /** Known child keys of a mapping. */
  keys?: Record<string, SchemaNode>;
  /** Schema of arbitrarily named children (service names, network names, ...). */
  any?: SchemaNode;
  /** Schema of sequence items. */
  list?: SchemaNode;
  /** Keys starting with this prefix are allowed anywhere and not completed (`x-`). */
  extensionPrefix?: string;
}

/** Custom YAML tags a file type understands (`!reset`), with Markdown documentation. */
export type SchemaTags = Readonly<Record<string, string>>;

export const key = (doc: string, type = "string", rest: Partial<SchemaNode> = {}): SchemaNode => ({ doc, type, ...rest });
export const flag = (doc: string, rest: Partial<SchemaNode> = {}): SchemaNode => ({ doc, type: "bool", values: ["true", "false"], ...rest });
export const map = (doc: string, keys: Record<string, SchemaNode>, rest: Partial<SchemaNode> = {}): SchemaNode => ({ doc, type: "map", keys, ...rest });
export const seq = (doc: string, item: SchemaNode = {}, rest: Partial<SchemaNode> = {}): SchemaNode => ({ doc, type: "list", list: item, ...rest });

interface Line {
  /** Column of the first `-` or key. */
  indent: number;
  /** Column of the key (after any `- `). */
  keyIndent: number;
  /** Column and length of the key text itself (inside the quotes for quoted keys). */
  keyStart: number;
  key: string | null;
  dash: boolean;
}

const KEY_LINE = /^(\s*)((?:-\s+)*)("([^"]+)"|'([^']+)'|[^\s:#{}[\],&*!|>'"%@`-][^:#]*?|-[^\s:#][^:#]*?)\s*:(?=\s|$)/;
const DASH_ONLY = /^(\s*)((?:-\s*)+)(?!\S*:)/;

function parseLine(text: string): Line | null {
  if (!text.trim() || /^\s*#/.test(text)) return null;
  const m = KEY_LINE.exec(text);
  if (m) {
    const keyIndent = m[1].length + m[2].length;
    const quoted = m[4] !== undefined || m[5] !== undefined;
    return {
      indent: m[1].length,
      keyIndent,
      keyStart: keyIndent + (quoted ? 1 : 0),
      key: m[4] ?? m[5] ?? m[3],
      dash: m[2].length > 0,
    };
  }
  const d = DASH_ONLY.exec(text);
  if (d) return { indent: d[1].length, keyIndent: d[1].length + d[2].length, keyStart: d[1].length + d[2].length, key: null, dash: true };
  return null;
}

/** An open key or sequence item; `id` is the line that introduced it. */
interface Entry {
  indent: number;
  key: string;
  id: number;
}

function advance(stack: Entry[], l: Line, lineNo: number): void {
  if (l.dash) {
    // A dash at the indent of its key (`tags:\n- x`) stays inside that key.
    while (stack.length && (stack[stack.length - 1].indent > l.indent || (stack[stack.length - 1].indent === l.indent && stack[stack.length - 1].key === "-"))) stack.pop();
    stack.push({ indent: l.indent, key: "-", id: lineNo });
  } else {
    while (stack.length && stack[stack.length - 1].indent >= l.keyIndent) stack.pop();
  }
  if (l.key !== null) {
    while (stack.length && stack[stack.length - 1].indent >= l.keyIndent) stack.pop();
    stack.push({ indent: l.keyIndent, key: l.key, id: lineNo });
  }
}

const ancestors = (stack: readonly Entry[], indent: number) => stack.filter((e) => e.indent < indent);

export interface Position {
  /** Keys leading to the mapping the cursor is in, with `-` for sequence items. */
  path: string[];
  /** Identifies that mapping instance (a list item differs from its siblings). */
  signature: string;
  /** The cursor is where a key (or sequence item) is written. */
  keyPosition: boolean;
  /** Key of the current line when the cursor is in its value. */
  currentKey: string | null;
  /** Column keys of this mapping are written at. */
  indent: number;
}

/** Where in the document structure `pos` is. */
export function positionAt(state: EditorState, pos: number): Position {
  const doc = state.doc;
  const cursorLine = doc.lineAt(pos);
  const before = cursorLine.text.slice(0, pos - cursorLine.from);
  const stack: Entry[] = [];
  for (let n = 1; n < cursorLine.number; n++) {
    const l = parseLine(doc.line(n).text);
    if (l) advance(stack, l, n);
  }
  const done = (indent: number, keyPosition: boolean, currentKey: string | null): Position => {
    const a = ancestors(stack, indent);
    return { path: a.map((e) => e.key), signature: a.map((e) => e.id).join("/"), keyPosition, currentKey, indent };
  };
  // `key: <cursor>` — a value (everything after the first `: ` belongs to it).
  const head = before.replace(/(:)\s+.*$/, "$1");
  const asKey = head !== before ? parseLine(head) : null;
  if (asKey?.key != null) {
    advance(stack, asKey, cursorLine.number);
    return done(asKey.keyIndent, false, asKey.key);
  }
  const ws = /^(\s*)((?:-\s+)*)/.exec(before)!;
  const indent = ws[1].length + ws[2].length;
  if (ws[2]) advance(stack, { indent: ws[1].length, keyIndent: indent, keyStart: indent, key: null, dash: true }, cursorLine.number);
  return done(indent, true, null);
}

/** The schema node at `path`, or null where the schema does not know the structure. */
export function nodeAt(root: SchemaNode, path: readonly string[]): SchemaNode | null {
  let node: SchemaNode | null = root;
  for (const k of path) {
    if (!node) return null;
    if (k === "-") {
      node = node.list ?? node;
      continue;
    }
    const prefix: string | undefined = node.extensionPrefix;
    if (prefix && k.startsWith(prefix)) return null;
    node = node.keys?.[k] ?? node.any ?? null;
  }
  return node;
}

function insertTextFor(name: string, node: SchemaNode, indent: number): string {
  const container = node.keys || node.any || node.list;
  if (container) return `${name}:\n${" ".repeat(indent + 2)}`;
  return `${name}: `;
}

export interface SchemaCompletionOptions {
  root: SchemaNode;
  /**
   * Completions the schema cannot know (service names, parameters, ...), offered at the
   * cursor's position in addition to the schema's own keys and values.
   */
  dynamic?: (at: Position, state: EditorState) => readonly Completion[] | null;
  /** Custom tags offered as the first token of any value. */
  tags?: SchemaTags;
}

/** Completion source for keys and values of a schema. */
export function schemaCompletion({ root, dynamic, tags = {} }: SchemaCompletionOptions) {
  return (ctx: CompletionContext): CompletionResult | null => {
    const pos = positionAt(ctx.state, ctx.pos);
    const line = ctx.state.doc.lineAt(ctx.pos);
    const before = line.text.slice(0, ctx.pos - line.from);
    if (/#/.test(before.replace(/(["']).*?\1/g, ""))) return null;
    if (pos.keyPosition) {
      const word = ctx.matchBefore(/[\w.-]*/);
      if (!word || (word.from === word.to && !ctx.explicit && !/(?:^|\s)$/.test(before))) return null;
      const node = nodeAt(root, pos.path);
      const present = siblingKeys(ctx.state, ctx.pos, pos);
      const options: Completion[] = Object.entries(node?.keys ?? {})
        .filter(([name]) => !present.has(name))
        .map(([name, child]) => ({
          label: name,
          type: child.keys || child.any ? "class" : child.list ? "enum" : "property",
          detail: child.type,
          info: child.doc ? () => docDom({ title: name, doc: child.doc! }) : undefined,
          apply: insertTextFor(name, child, pos.indent),
        }));
      options.push(...(dynamic?.(pos, ctx.state) ?? []));
      return options.length ? { from: word.from, options, validFor: /^[\w.-]*$/ } : null;
    }
    // Value position.
    const tagNames = Object.keys(tags);
    const word = ctx.matchBefore(tagNames.length ? /!?[\w.@/-]*/ : /[\w.@/-]*/);
    if (!word) return null;
    const node = pos.currentKey ? nodeAt(root, [...pos.path, pos.currentKey]) : null;
    const fixed: Completion[] = (node?.values ?? []).map((v) => ({ label: v, type: "constant" }));
    // A tag is only valid as the first token of the value: `ports: !reset []`.
    const firstToken = pos.currentKey !== null && /:\s+$/.test(before.slice(0, word.from - line.from));
    const tagged: Completion[] =
      firstToken && (word.text === "" || word.text.startsWith("!"))
        ? tagNames.map((label) => ({ label, type: "keyword", detail: "tag", info: () => docDom({ title: label, doc: tags[label] }) }))
        : [];
    const options = word.text.startsWith("!") ? tagged : [...(dynamic?.(pos, ctx.state) ?? []), ...fixed, ...tagged];
    if (!options.length) return null;
    return { from: word.from, options, validFor: tagNames.length ? /^!?[\w.@/-]*$/ : /^[\w.@/-]*$/ };
  };
}

/** Keys already written in the mapping the cursor is in. */
function siblingKeys(state: EditorState, pos: number, at: Position): Set<string> {
  const doc = state.doc;
  const cursor = doc.lineAt(pos).number;
  const out = new Set<string>();
  const stack: Entry[] = [];
  for (let n = 1; n <= doc.lines; n++) {
    const l = parseLine(doc.line(n).text);
    if (!l) continue;
    advance(stack, l, n);
    if (n === cursor || l.key === null || l.keyIndent !== at.indent) continue;
    if (ancestors(stack, l.keyIndent).map((e) => e.id).join("/") === at.signature) out.add(l.key);
  }
  return out;
}

/** Hover documentation of the key (or custom tag) under the pointer. */
export function schemaHover(root: SchemaNode, tags: SchemaTags = {}): Extension {
  return hoverDocs((state, pos) => schemaHoverAt(root, tags, state, pos));
}

/** The hover of `schemaHover` at `pos`: a known custom tag, or a key of the schema. */
export function schemaHoverAt(root: SchemaNode, tags: SchemaTags, state: EditorState, pos: number): HoverHit | null {
  const tree = syntaxTree(state);
  const tag = [tree.resolveInner(pos, -1), tree.resolveInner(pos, 1)].find((n) => n.name === "Tag");
  if (tag) {
    const name = state.sliceDoc(tag.from, tag.to);
    const doc = tags[name];
    return doc ? { from: tag.from, to: tag.to, dom: () => docDom({ title: name, doc }) } : null;
  }
  const line = state.doc.lineAt(pos);
  const l = parseLine(line.text);
  if (!l?.key) return null;
  const from = line.from + l.keyStart;
  const to = from + l.key.length;
  if (pos < from || pos > to) return null;
  const p = positionAt(state, from);
  const node = nodeAt(root, [...p.path, l.key]);
  if (!node?.doc) return null;
  const title = node.type ? `${l.key}: ${node.type}` : l.key;
  return { from, to, dom: () => docDom({ title, doc: node.doc! }) };
}
