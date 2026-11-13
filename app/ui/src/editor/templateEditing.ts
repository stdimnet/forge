/**
 * Editing support for template tags that follows the file's delimiters (`templateSyntaxField`):
 * comment toggling (`{* *}` or `[[* *]]`), folding of blocks and comments, highlighting of the
 * matching delimiter and block tag, auto-closing of tags and completion of tag names.
 */
import type { CompletionContext, CompletionResult } from "@codemirror/autocomplete";
import { foldService } from "@codemirror/language";
import { EditorState, Prec, type EditorState as State, type Extension } from "@codemirror/state";
import { Decoration, EditorView, ViewPlugin, type DecorationSet, type ViewUpdate } from "@codemirror/view";
import { scanDoc, templateSyntaxField, type TemplateTag } from "./templateOverlay";
import type { TemplateSyntax } from "./templates";

export interface TemplateBlock {
  open: TemplateTag;
  close: TemplateTag;
}

/** Pairs block tags with their end tags (`{if}` … `{/if}`); tags without an end tag are skipped. */
export function templateBlocks(tags: TemplateTag[]): TemplateBlock[] {
  const stack: TemplateTag[] = [];
  const blocks: TemplateBlock[] = [];
  for (const tag of tags) {
    if (!tag.name) continue;
    if (tag.kind === "block") stack.push(tag);
    else if (tag.kind === "end") {
      const i = stack.map((t) => t.name).lastIndexOf(tag.name);
      if (i < 0) continue;
      blocks.push({ open: stack[i], close: tag });
      stack.length = i;
    }
  }
  return blocks;
}

/** The comment delimiters of a syntax: the first comment pair, e.g. `[[*` `*]]`. */
export function commentDelimiters(syntax: TemplateSyntax): { open: string; close: string } | undefined {
  const d = syntax.delimiters.find(([, , kind]) => kind === "comment");
  return d && { open: d[0], close: d[1] };
}

const syntaxOf = (state: State) => state.field(templateSyntaxField, false) ?? null;

/** Block names that are open at `pos`, innermost last. */
export function openBlocksAt(tags: TemplateTag[], pos: number): string[] {
  const stack: string[] = [];
  for (const tag of tags) {
    if (tag.to > pos) break;
    if (!tag.name) continue;
    if (tag.kind === "block") stack.push(tag.name);
    else if (tag.kind === "end") {
      const i = stack.lastIndexOf(tag.name);
      if (i >= 0) stack.length = i;
    }
  }
  return stack;
}

/** Completion of tag names after an opener (`[[fo` → `foreach`) and of end tags (`[[/` → `if]]`). */
export function templateTagCompletion(ctx: CompletionContext): CompletionResult | null {
  const syntax = syntaxOf(ctx.state);
  if (!syntax) return null;
  const word = ctx.matchBefore(/[A-Za-z_]\w*$/);
  const from = word?.from ?? ctx.pos;
  const before = ctx.state.sliceDoc(Math.max(0, from - 12), from);
  const opener = syntax.delimiters.find(([open, , kind]) => kind !== "comment" && before.endsWith(open));
  if (!opener) return null;
  const [, close, kind] = opener;
  const after = ctx.state.sliceDoc(ctx.pos, ctx.pos + close.length);
  const closing = after === close ? "" : close;
  if (kind === "end") {
    const open = openBlocksAt(scanDoc(ctx.state.doc, syntax).tags, from).reverse();
    const names = [...new Set(open)];
    if (!names.length) return null;
    return {
      from,
      options: names.map((name, i) => ({ label: name, type: "keyword", boost: names.length - i, apply: `${name}${closing}` })),
      validFor: /^\w*$/,
    };
  }
  if (!word && !ctx.explicit) return null;
  // `{% end…` closes the innermost open block first (`endfor`, then `endif`).
  const ends = syntax.endPrefix
    ? [...new Set(openBlocksAt(scanDoc(ctx.state.doc, syntax).tags, from).reverse())].map((n) => syntax.endPrefix + n)
    : [];
  return {
    from,
    options: [
      ...ends.map((label, i) => ({ label, type: "keyword", boost: 50 - i })),
      ...syntax.keywords.filter((k) => !ends.includes(k)).map((label) => ({ label, type: "keyword" })),
    ],
    validFor: /^\w*$/,
  };
}

/**
 * Typing the last character of an opener inserts the closer (`<%` → `<%|%>`), and turning
 * `[[|]]` into a comment opener completes the comment (`[[*|*]]`). Openers made of brackets
 * (`{`, `{{`, `[[`) get their closer from `closeBrackets`, one bracket at a time.
 */
export const templateAutoClose = EditorView.inputHandler.of((view, from, to, text) => {
  const syntax = syntaxOf(view.state);
  if (!syntax || text.length !== 1 || from !== to) return false;
  const line = view.state.doc.lineAt(from);
  const before = line.text.slice(0, from - line.from) + text;
  const after = line.text.slice(to - line.from);
  for (const [open, close, kind] of syntax.delimiters) {
    if (!before.endsWith(open)) continue;
    const plain = syntax.delimiters.find(([o, , k]) => k !== "comment" && o.length < open.length && open.startsWith(o));
    if (kind === "comment" && plain && after.startsWith(plain[1]) && close.endsWith(plain[1])) {
      // `[[|]]` + `*` → `[[*|*]]`: the plain closer becomes the comment closer.
      const insert = text + close.slice(0, close.length - plain[1].length);
      view.dispatch({ changes: { from, insert }, selection: { anchor: from + 1 }, userEvent: "input.type" });
      return true;
    }
    // `{{` and `[[` already get their closer from closeBrackets, one bracket at a time.
    if (kind !== "comment" && open.length > 1 && !plain && !"([{".includes(text) && !after.startsWith(close)) {
      view.dispatch({ changes: { from, insert: text + close }, selection: { anchor: from + 1 }, userEvent: "input.type" });
      return true;
    }
    return false;
  }
  return false;
});

/** Folds block tags (`[[foreach …]]` … `[[/foreach]]`) and comments that span lines. */
export const templateFolding = foldService.of((state, lineStart, lineEnd) => {
  const syntax = syntaxOf(state);
  if (!syntax) return null;
  const { tags } = scanDoc(state.doc, syntax);
  for (const { open, close } of templateBlocks(tags)) {
    if (open.to >= lineStart && open.to <= lineEnd && close.from > lineEnd) return { from: open.to, to: close.from };
  }
  for (const tag of tags) {
    if (tag.kind === "comment" && tag.from >= lineStart && tag.from <= lineEnd && tag.to > lineEnd) {
      return { from: tag.openEnd, to: tag.closeStart };
    }
  }
  return null;
});

const matchMark = Decoration.mark({ class: "cm-matchingBracket" });

/** Ranges to highlight for the cursor at `pos`: the tag's delimiters and those of its block partner. */
export function matchingDelimiters(tags: TemplateTag[], pos: number): [number, number][] {
  const tag = tags.find((t) => (pos >= t.from && pos <= t.openEnd) || (pos >= t.closeStart && pos <= t.to));
  if (!tag) return [];
  const out: [number, number][] = [
    [tag.from, tag.openEnd],
    [tag.closeStart, tag.to],
  ];
  const block = templateBlocks(tags).find((b) => b.open === tag || b.close === tag);
  const partner = block && (block.open === tag ? block.close : block.open);
  if (partner) out.push([partner.from, partner.openEnd], [partner.closeStart, partner.to]);
  return out.sort((a, b) => a[0] - b[0]);
}

const templateMatching = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet = Decoration.none;
    constructor(view: EditorView) {
      this.decorations = this.compute(view.state);
    }
    update(u: ViewUpdate) {
      if (u.docChanged || u.selectionSet || u.startState.field(templateSyntaxField, false) !== u.state.field(templateSyntaxField, false)) {
        this.decorations = this.compute(u.state);
      }
    }
    compute(state: State): DecorationSet {
      const syntax = syntaxOf(state);
      if (!syntax || !state.selection.main.empty) return Decoration.none;
      const ranges = matchingDelimiters(scanDoc(state.doc, syntax).tags, state.selection.main.head);
      return Decoration.set(ranges.map(([from, to]) => matchMark.range(from, to)));
    }
  },
  { decorations: (v) => v.decorations },
);

/** Comment tokens of the template (`[[* *]]`) take precedence over the data language's. */
const templateLanguageData = Prec.highest(
  EditorState.languageData.of((state) => {
    const syntax = syntaxOf(state);
    const comment = syntax && commentDelimiters(syntax);
    return [{ ...(comment ? { commentTokens: { block: comment } } : {}), autocomplete: templateTagCompletion }];
  }),
);

/** Editing features for a template file; needs `templateOverlay` for the syntax field. */
export function templateEditing(): Extension {
  return [templateLanguageData, templateAutoClose, templateFolding, templateMatching];
}
