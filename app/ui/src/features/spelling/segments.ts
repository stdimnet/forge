/**
 * What to spell check in a document range: comments, doc comments, string literals and
 * declared identifiers, found with the editor's own syntax highlighting (the Lezer tree of
 * any language, legacy modes included), or the prose of Markdown and plain text files
 * without code, links and markup. Rust (`forge-spell`) splits them into words.
 */
import { ensureSyntaxTree, syntaxTree } from "@codemirror/language";
import type { EditorState } from "@codemirror/state";
import { highlightTree, tags, type Highlighter, type Tag } from "@lezer/highlight";
import type { SpellContext, SpellSegment } from "@/ipc/spell";

/** Tags of declarations: their names are spell checked (references are not). */
const DEFINITIONS = [
  tags.definition(tags.variableName),
  tags.definition(tags.propertyName),
  tags.definition(tags.className),
  tags.definition(tags.typeName),
  tags.definition(tags.name),
  tags.definition(tags.labelName),
  tags.definition(tags.namespace),
];

type Kind = "comment" | "docComment" | "string" | "identifier" | "variable";

function kindOf(tagList: readonly Tag[]): Kind | null {
  for (const t of tagList) {
    const set = t.set;
    // Regular expressions and escape sequences are kinds of strings, but not prose.
    if (set.includes(tags.regexp) || set.includes(tags.escape)) return null;
    if (set.includes(tags.docComment)) return "docComment";
    if (set.includes(tags.comment)) return "comment";
    if (set.includes(tags.string) || set.includes(tags.docString) || set.includes(tags.character)) return "string";
    if (DEFINITIONS.some((d) => set.includes(d))) return "identifier";
    if (t === tags.variableName) return "variable";
  }
  return null;
}

const codeHighlighter: Highlighter = { style: (t) => kindOf(t) };

/** Markdown and plain text: the prose, without code, URLs and markup characters. */
const SKIP_IN_PROSE = new Set([
  "FencedCode",
  "CodeBlock",
  "InlineCode",
  "CodeText",
  "HTMLBlock",
  "HTMLTag",
  "URL",
  "Autolink",
  "LinkReference",
  "LinkLabel",
  "Comment",
  "CommentBlock",
  "ProcessingInstructionBlock",
]);

export function isProseFile(typeId: string | null | undefined): boolean {
  return typeId == null || typeId === "markdown";
}

/** The tree up to `to`, parsing a little more when the parser has not got there yet. */
function treeTo(state: EditorState, to: number) {
  return ensureSyntaxTree(state, to, 50) ?? syntaxTree(state);
}

function codeSegments(state: EditorState, from: number, to: number, out: SpellSegment[]): void {
  const tree = treeTo(state, to);
  let last: SpellSegment | null = null;
  highlightTree(
    tree,
    codeHighlighter,
    (start, end, cls) => {
      let kind = cls as Kind;
      const text = state.sliceDoc(start, end);
      if (kind === "variable") {
        // Plain variable references are only checked for `$name` variables (PHP), whose
        // first use declares them.
        if (!text.startsWith("$")) return;
        kind = "identifier";
      }
      if (kind === "comment" && /^\/\*\*(?!\/)/.test(text)) kind = "docComment";
      // Pieces of one comment or string (split by nested nodes) stay one segment.
      if (last && last.kind === kind && kind !== "identifier" && last.from + last.text.length === start) {
        last.text += text;
        return;
      }
      last = { from: start, text, kind: kind as SpellContext };
      out.push(last);
    },
    from,
    to,
  );
}

function proseSegments(state: EditorState, from: number, to: number, out: SpellSegment[]): void {
  const skipped: [number, number][] = [];
  const tree = treeTo(state, to);
  tree.iterate({
    from,
    to,
    enter: (node) => {
      if (SKIP_IN_PROSE.has(node.name)) {
        skipped.push([node.from, node.to]);
        return false;
      }
      // Markup characters (`#`, `*`, `>`, `[`) are not words but do not split sentences.
      return undefined;
    },
  });
  skipped.sort((a, b) => a[0] - b[0]);
  let at = from;
  for (const [f, t] of skipped) {
    if (f > at) out.push({ from: at, text: state.sliceDoc(at, Math.min(f, to)), kind: "text" });
    at = Math.max(at, t);
    if (at >= to) break;
  }
  if (at < to) out.push({ from: at, text: state.sliceDoc(at, to), kind: "text" });
}

/** The segments of `ranges` (sorted, not overlapping). */
export function collectSegments(state: EditorState, ranges: readonly [number, number][], prose: boolean): SpellSegment[] {
  const out: SpellSegment[] = [];
  for (const [from, to] of ranges) {
    if (to <= from) continue;
    if (prose) proseSegments(state, from, to, out);
    else codeSegments(state, from, to, out);
  }
  return out.filter((s) => /\p{L}/u.test(s.text));
}

const BLOCK_NODE = /Comment|String|Template|Heredoc|Nowdoc|Docstring|FencedCode|CodeBlock|Paragraph/;

/**
 * A changed range grown to whole lines and to the comments or strings it touches, so
 * multi-line comments are checked as a whole.
 */
export function expandRange(state: EditorState, from: number, to: number): [number, number] {
  const doc = state.doc;
  let f = doc.lineAt(Math.min(from, doc.length)).from;
  let t = doc.lineAt(Math.min(to, doc.length)).to;
  const tree = syntaxTree(state);
  for (const [pos, side] of [
    [f, 1],
    [t, -1],
  ] as const) {
    for (let node: ReturnType<typeof tree.resolveInner> | null = tree.resolveInner(pos, side); node; node = node.parent) {
      if (BLOCK_NODE.test(node.name)) {
        f = Math.min(f, node.from);
        t = Math.max(t, node.to);
        break;
      }
    }
  }
  return [f, t];
}

/** Sorted, merged ranges. */
export function mergeRanges(ranges: [number, number][]): [number, number][] {
  const sorted = [...ranges].sort((a, b) => a[0] - b[0]);
  const out: [number, number][] = [];
  for (const r of sorted) {
    const last = out[out.length - 1];
    if (last && r[0] <= last[1]) last[1] = Math.max(last[1], r[1]);
    else out.push([r[0], r[1]]);
  }
  return out;
}
