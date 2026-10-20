/**
 * Template tags as a decoration layer over the data language. The file keeps the highlighting
 * of its real language (HTML, CSS, SQL, ...) from that language's own parser; this layer only
 * colors what belongs to the template engine: delimiters, keywords, variables, filters,
 * strings and comments inside `{{ }}`, `{% %}` and friends. Tags may sit anywhere in the
 * data language, even where its parser would not expect them.
 */
import { StringStream } from "@codemirror/language";
import { Compartment, Prec, RangeSetBuilder, StateEffect, StateField, type Extension, type Text } from "@codemirror/state";
import { Decoration, EditorView, ViewPlugin, type DecorationSet, type ViewUpdate } from "@codemirror/view";
import { createEffect, createRoot } from "solid-js";
import { templateParser, type TagKind, type TemplateSyntax } from "./templates";

export interface TemplateRange {
  from: number;
  to: number;
  /** Token style of the template parser, e.g. `keyword` or `variableName.function`. */
  style: string;
}

/** One template tag: `[from, openEnd)` is its opening delimiter, `[closeStart, to)` its closing one. */
export interface TemplateTag {
  from: number;
  openEnd: number;
  closeStart: number;
  to: number;
  kind: TagKind;
  /** First word of a block or end tag (`if` in `{if …}` and `{/if}`). */
  name?: string;
}

/** Documents above this size keep only their data language highlighting. */
export const OVERLAY_MAX_CHARS = 400_000;

/** Template tokens and tags of `text`; data language text is left out. */
export function scanTemplate(text: string, syntax: TemplateSyntax): { ranges: TemplateRange[]; tags: TemplateTag[] } {
  // With no data language the parser styles only template tokens and leaves the rest unstyled.
  const parser = templateParser(syntax, null);
  const state = parser.startState!(2);
  const ranges: TemplateRange[] = [];
  const tags: TemplateTag[] = [];
  let open: Omit<TemplateTag, "closeStart" | "to"> | null = null;
  let offset = 0;
  for (const line of text.split("\n")) {
    if (line === "") parser.blankLine?.(state, 2);
    const stream = new StringStream(line, 4, 2);
    while (!stream.eol()) {
      stream.start = stream.pos;
      const before = state.inTag;
      const style = parser.token(stream, state);
      if (stream.pos === stream.start) stream.next();
      if (style) ranges.push({ from: offset + stream.start, to: offset + stream.pos, style });
      if (!before && state.inTag) {
        open = { from: offset + stream.start, openEnd: offset + stream.pos, kind: state.inTag.kind };
      } else if (before && !state.inTag && open) {
        const to = offset + stream.pos;
        const closeStart = before.kind === "comment" ? to - before.close.length : offset + stream.start;
        tags.push({ ...open, closeStart, to, name: before.name });
        open = null;
      }
    }
    offset += line.length + 1;
  }
  return { ranges, tags };
}

/** Offsets and styles of the template tokens in `text`; data language text is left out. */
export const templateRanges = (text: string, syntax: TemplateSyntax): TemplateRange[] => scanTemplate(text, syntax).ranges;

const scans = new WeakMap<Text, { syntax: TemplateSyntax; scan: ReturnType<typeof scanTemplate> }>();

/** `scanTemplate` of a document, cached per document and syntax. */
export function scanDoc(doc: Text, syntax: TemplateSyntax): ReturnType<typeof scanTemplate> {
  const hit = scans.get(doc);
  if (hit && hit.syntax === syntax) return hit.scan;
  const scan = doc.length > OVERLAY_MAX_CHARS ? { ranges: [], tags: [] } : scanTemplate(doc.toString(), syntax);
  scans.set(doc, { syntax, scan });
  return scan;
}

/** CSS class of a token style, defined in `overlayTheme`. */
export const overlayClass = (style: string): string => `cm-tpl-${style.replace(/\./g, "-")}`;

function decorate(doc: Text, syntax: TemplateSyntax): DecorationSet {
  const builder = new RangeSetBuilder<Decoration>();
  for (const r of scanDoc(doc, syntax).ranges) {
    builder.add(r.from, r.to, Decoration.mark({ class: `cm-tpl ${overlayClass(r.style)}` }));
  }
  return builder.finish();
}

/** The colors follow the editor's `--syn-*` tokens, like `forgeHighlight`. */
const overlayTheme = EditorView.baseTheme({
  ".cm-tpl-meta": { color: "var(--syn-attribute)" },
  ".cm-tpl-comment": { color: "var(--syn-comment)", fontStyle: "italic" },
  ".cm-tpl-keyword": { color: "var(--syn-keyword)" },
  ".cm-tpl-string": { color: "var(--syn-string)" },
  ".cm-tpl-number": { color: "var(--syn-number)" },
  ".cm-tpl-atom": { color: "var(--syn-number)" },
  ".cm-tpl-variableName": { color: "var(--syn-variable)" },
  ".cm-tpl-variableName-function": { color: "var(--syn-function)" },
  ".cm-tpl-propertyName": { color: "var(--syn-attribute)" },
  ".cm-tpl-operator": { color: "var(--syn-operator)" },
  ".cm-tpl-punctuation": { color: "var(--syn-operator)" },
});

/** Replaces the template syntax of an editor, e.g. after the delimiter settings changed. */
export const setTemplateSyntax = StateEffect.define<TemplateSyntax>();

/** The template syntax of the editor's file; editing features read their delimiters from it. */
export const templateSyntaxField = StateField.define<TemplateSyntax | null>({
  create: () => null,
  update: (value, tr) => {
    for (const e of tr.effects) if (e.is(setTemplateSyntax)) value = e.value;
    return value;
  },
});

const sameSyntax = (a: TemplateSyntax | null, b: TemplateSyntax) =>
  a != null && a.name === b.name && JSON.stringify(a.delimiters) === JSON.stringify(b.delimiters);

/**
 * The template overlay. With a function the syntax is live: it is re-read when the settings it
 * depends on change (delimiters), and open editors are highlighted again.
 *
 * @param dataLanguage The data language's highlighting when it depends on the syntax (a stream
 *   parser that skips the template tags); it is replaced together with the syntax.
 */
export function templateOverlay(
  syntax: TemplateSyntax | (() => TemplateSyntax),
  dataLanguage?: (syntax: TemplateSyntax) => Extension,
): Extension {
  const resolve = typeof syntax === "function" ? syntax : () => syntax;
  const language = new Compartment();
  const initial = resolve();
  const plugin = ViewPlugin.define(
    (view) => {
      let current = view.state.field(templateSyntaxField);
      let decorations = current ? decorate(view.state.doc, current) : Decoration.none;
      let destroyed = false;
      // Follow the settings behind `resolve`; Solid tracks the `getSetting` reads.
      const dispose =
        typeof syntax === "function"
          ? createRoot((dispose) => {
              createEffect(() => {
                const next = resolve();
                if (sameSyntax(view.state.field(templateSyntaxField), next)) return;
                queueMicrotask(() => {
                  if (destroyed) return;
                  const effects: StateEffect<unknown>[] = [setTemplateSyntax.of(next)];
                  if (dataLanguage) effects.push(language.reconfigure(dataLanguage(next)));
                  view.dispatch({ effects });
                });
              });
              return dispose;
            })
          : undefined;
      return {
        get decorations() {
          return decorations;
        },
        update(u: ViewUpdate) {
          const next = u.state.field(templateSyntaxField);
          if (u.docChanged || next !== current) {
            current = next;
            decorations = next ? decorate(u.state.doc, next) : Decoration.none;
          }
        },
        destroy() {
          destroyed = true;
          dispose?.();
        },
      };
    },
    { decorations: (v) => v.decorations },
  );
  // Above the data language's own highlighting, so the tag colors win where they overlap.
  return [
    templateSyntaxField.init(() => initial),
    dataLanguage ? language.of(dataLanguage(initial)) : [],
    Prec.high(plugin),
    overlayTheme,
  ];
}
