/**
 * Rendered documentation comments (PHPStorm's "Toggle Rendered View", Ctrl+Alt+Q): a
 * PHPDoc / JSDoc / LuaDoc comment is shown as formatted text, with its tags in a table
 * (`Params:`, `Returns:`, `Throws:`) and clickable links, in place of the comment. Each
 * comment has a gutter icon that toggles it; "Render All Doc Comments" toggles the file,
 * the setting `editor.renderDocComments` (and Reader mode) is the default. Comments are
 * found and parsed in Rust (`forge_markdown::doc_comments`); this module only draws them.
 */
import { Facet, StateEffect, StateField, type EditorState, type Extension, type Range } from "@codemirror/state";
import { Decoration, EditorView, WidgetType, type DecorationSet } from "@codemirror/view";
import { PencilLine } from "lucide-solid";
import { buildHoverDom } from "@/features/lsp/hoverDoc";
import { iconNode } from "@/features/lsp/util";
import { docCommentsOf, type DocCommentInfo } from "./model";

/** Languages whose documentation comments Forge renders (see `forge_markdown::doc_style`). */
export const DOC_LANGUAGES = new Set(["php", "blade", "javascript", "jsx", "typescript", "tsx", "vue", "lua"]);

/** Render every comment of the editor by default (true) or none (false). */
export const setRenderAll = StateEffect.define<boolean>();
/** Toggle the comment that starts at this document offset. */
export const toggleRendered = StateEffect.define<number>();

/** Recompute the decorations (the module loaded after the editor opened). */
export const refreshDocs = StateEffect.define<null>();

const docLanguage = Facet.define<string, string>({ combine: (v) => v[0] ?? "" });

interface RenderState {
  all: boolean;
  /** Start offsets of comments rendered differently from `all`. */
  flipped: ReadonlySet<number>;
  deco: DecorationSet;
}

const signatureLanguage = (language: string) => (language === "lua" ? "lua" : language === "php" || language === "blade" ? "php" : "typescript");

class RenderedDoc extends WidgetType {
  readonly key: string;

  constructor(
    readonly info: DocCommentInfo,
    readonly language: string,
  ) {
    super();
    this.key = `${info.column}:${JSON.stringify(info.doc)}`;
  }

  eq(other: RenderedDoc): boolean {
    return other.key === this.key;
  }

  toDOM(view: EditorView): HTMLElement {
    const wrap = document.createElement("div");
    wrap.className = "cm-rendered-doc";
    wrap.style.marginLeft = `${this.info.column}ch`;
    // Back to the comment's text (PHPStorm shows this toggle in the gutter).
    const edit = document.createElement("button");
    edit.className = "cm-rendered-doc-edit";
    edit.title = "Toggle Rendered View";
    edit.append(iconNode(PencilLine, 13));
    edit.addEventListener("mousedown", (e) => {
      e.preventDefault();
      e.stopPropagation();
      view.dispatch({ effects: toggleRendered.of(commentStart(view.state, this.info)) });
    });
    wrap.append(edit, buildHoverDom(this.info.doc, [], signatureLanguage(this.language)));
    wrap.addEventListener("click", (e) => {
      const link = (e.target as Element).closest(".md-link") as HTMLElement | null;
      const href = link?.title;
      if (href && /^https?:\/\//.test(href)) {
        e.preventDefault();
        window.open(href, "_blank", "noopener");
      }
    });
    return wrap;
  }

  ignoreEvent(): boolean {
    return true;
  }
}

const isEmpty = (c: DocCommentInfo) => !c.doc.body.length && !c.doc.sections.length && c.doc.deprecated === undefined;

/** The start offset of a comment (the key of `flipped`). */
export const commentStart = (state: EditorState, c: DocCommentInfo): number => state.doc.line(c.line + 1).from;

function decorations(state: EditorState, all: boolean, flipped: ReadonlySet<number>): DecorationSet {
  // Nothing rendered: no need to parse the comments on every keystroke.
  if (!all && !flipped.size) return Decoration.none;
  const language = state.facet(docLanguage);
  const ranges: Range<Decoration>[] = [];
  for (const c of docCommentsOf(state.doc, language)) {
    if (c.endLine >= state.doc.lines || isEmpty(c)) continue;
    const from = commentStart(state, c);
    if (all === flipped.has(from)) continue;
    const to = state.doc.line(c.endLine + 1).to;
    ranges.push(Decoration.replace({ widget: new RenderedDoc(c, language), block: true }).range(from, to));
  }
  return Decoration.set(ranges, true);
}

export const renderedDocsField = StateField.define<RenderState>({
  create: () => ({ all: false, flipped: new Set(), deco: Decoration.none }),
  update(value, tr) {
    let { all, flipped } = value;
    if (tr.docChanged && flipped.size) flipped = new Set([...flipped].map((o) => tr.changes.mapPos(o)));
    for (const e of tr.effects) {
      if (e.is(setRenderAll)) {
        all = e.value;
        flipped = new Set();
      } else if (e.is(toggleRendered)) {
        const next = new Set(flipped);
        if (next.has(e.value)) next.delete(e.value);
        else next.add(e.value);
        flipped = next;
      }
    }
    if (!tr.docChanged && all === value.all && flipped === value.flipped && !tr.effects.some((e) => e.is(refreshDocs))) return value;
    return { all, flipped, deco: decorations(tr.state, all, flipped) };
  },
  provide: (f) => EditorView.decorations.from(f, (v) => v.deco),
});

const docsTheme = EditorView.baseTheme({
  ".cm-rendered-doc": {
    position: "relative",
    margin: "2px 0 4px",
    borderLeft: "2px solid var(--border-strong)",
    backgroundColor: "color-mix(in srgb, var(--bg-hover) 45%, transparent)",
    borderRadius: "0 var(--radius) var(--radius) 0",
    cursor: "default",
  },
  ".cm-rendered-doc .lsp-hover": { maxWidth: "720px", maxHeight: "none", overflow: "visible" },
  ".cm-rendered-doc .lsp-hover-doc": { padding: "4px 12px 6px" },
  ".cm-rendered-doc .lsp-hover-body": { marginTop: "0", paddingTop: "0", borderTop: "none" },
  ".cm-rendered-doc .lsp-hover-body p": { margin: "2px 0" },
  ".cm-rendered-doc .lsp-hover-sections": { marginTop: "4px" },
  ".cm-rendered-doc .md-link": { cursor: "pointer" },
  ".cm-rendered-doc-edit": {
    position: "absolute",
    top: "4px",
    right: "6px",
    display: "inline-flex",
    padding: "2px",
    border: "none",
    borderRadius: "3px",
    background: "transparent",
    color: "var(--fg-faint)",
    cursor: "pointer",
    visibility: "hidden",
  },
  ".cm-rendered-doc:hover .cm-rendered-doc-edit": { visibility: "visible" },
  ".cm-rendered-doc-edit:hover": { color: "var(--accent)", background: "var(--bg-hover)" },
});

/** Rendered doc comments for an editor of `language`; `initial` renders all at the start. */
export function renderedDocs(language: string, initial: (state: EditorState) => boolean): Extension {
  return [
    docLanguage.of(language),
    renderedDocsField.init((state) => {
      const all = initial(state);
      return { all, flipped: new Set(), deco: all ? decorations(state, true, new Set()) : Decoration.none };
    }),
    docsTheme,
  ];
}

/** The comment at (or right above) a 0-based line. */
export function docCommentAtLine(state: EditorState, line: number): DocCommentInfo | undefined {
  const comments = docCommentsOf(state.doc, state.facet(docLanguage));
  return (
    comments.find((c) => c.line <= line && c.endLine >= line) ??
    comments.find((c) => c.endLine === line - 1) ??
    // On a declaration line below attributes: the nearest comment above within 3 lines.
    [...comments].reverse().find((c) => c.endLine < line && line - c.endLine <= 3)
  );
}

/** Toggle the rendered view of the comment at or above `line`; false when there is none. */
export function toggleDocAtLine(view: EditorView, line: number): boolean {
  if (!view.state.field(renderedDocsField, false)) return false;
  const c = docCommentAtLine(view.state, line);
  if (!c) return false;
  view.dispatch({ effects: toggleRendered.of(commentStart(view.state, c)) });
  return true;
}

/** Whether all comments of the editor are rendered by default. */
export const allRendered = (state: EditorState): boolean => state.field(renderedDocsField, false)?.all ?? false;

/** The fold ranges of the documentation comments (Collapse Doc Comments). */
export function docCommentFolds(state: EditorState): { from: number; to: number }[] {
  return docCommentsOf(state.doc, state.facet(docLanguage))
    .filter((c) => c.endLine > c.line && c.endLine < state.doc.lines)
    .map((c) => ({ from: state.doc.line(c.line + 1).to, to: state.doc.line(c.endLine + 1).to }));
}
