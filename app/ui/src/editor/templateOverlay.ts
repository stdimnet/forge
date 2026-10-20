/**
 * Template tags as a decoration layer over the data language. The file keeps the highlighting
 * of its real language (HTML, CSS, SQL, ...) from that language's own parser; this layer only
 * colors what belongs to the template engine: delimiters, keywords, variables, filters,
 * strings and comments inside `{{ }}`, `{% %}` and friends. Tags may sit anywhere in the
 * data language, even where its parser would not expect them.
 */
import { StringStream } from "@codemirror/language";
import { Prec, RangeSetBuilder, type Extension, type Text } from "@codemirror/state";
import { Decoration, EditorView, ViewPlugin, type DecorationSet, type ViewUpdate } from "@codemirror/view";
import { templateParser, type TemplateSyntax } from "./templates";

export interface TemplateRange {
  from: number;
  to: number;
  /** Token style of the template parser, e.g. `keyword` or `variableName.function`. */
  style: string;
}

/** Documents above this size keep only their data language highlighting. */
export const OVERLAY_MAX_CHARS = 400_000;

/** Offsets and styles of the template tokens in `text`; data language text is left out. */
export function templateRanges(text: string, syntax: TemplateSyntax): TemplateRange[] {
  // With no data language the parser styles only template tokens and leaves the rest unstyled.
  const parser = templateParser(syntax, null);
  const state = parser.startState!(2);
  const out: TemplateRange[] = [];
  let offset = 0;
  for (const line of text.split("\n")) {
    if (line === "") parser.blankLine?.(state, 2);
    const stream = new StringStream(line, 4, 2);
    while (!stream.eol()) {
      stream.start = stream.pos;
      const style = parser.token(stream, state);
      if (stream.pos === stream.start) stream.next();
      if (style) out.push({ from: offset + stream.start, to: offset + stream.pos, style });
    }
    offset += line.length + 1;
  }
  return out;
}

/** CSS class of a token style, defined in `overlayTheme`. */
export const overlayClass = (style: string): string => `cm-tpl-${style.replace(/\./g, "-")}`;

function decorate(doc: Text, syntax: TemplateSyntax): DecorationSet {
  if (doc.length > OVERLAY_MAX_CHARS) return Decoration.none;
  const builder = new RangeSetBuilder<Decoration>();
  for (const r of templateRanges(doc.toString(), syntax)) {
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

export function templateOverlay(syntax: TemplateSyntax): Extension {
  const plugin = ViewPlugin.define(
    (view) => {
      let decorations = decorate(view.state.doc, syntax);
      return {
        get decorations() {
          return decorations;
        },
        update(u: ViewUpdate) {
          if (u.docChanged) decorations = decorate(u.state.doc, syntax);
        },
      };
    },
    { decorations: (v) => v.decorations },
  );
  // Above the data language's own highlighting, so the tag colors win where they overlap.
  return [Prec.high(plugin), overlayTheme];
}
