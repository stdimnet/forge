/**
 * Emmet in the UI: the settings as Rust expects them and applying an expansion (text + tab
 * stops, computed in Rust) as a CodeMirror snippet, like a live template.
 */
import { EditorSelection } from "@codemirror/state";
import type { EditorView } from "@codemirror/view";
import { indentUnit } from "@codemirror/language";
import { snippet } from "@codemirror/autocomplete";
import { getSetting } from "@/state/settings";
import { languageIdFor } from "@/editor/languages";
import { toSnippet } from "@/features/liveTemplates/model";
import type { EmmetEdit, EmmetExpansion, EmmetOptions } from "@/ipc/emmet";

export const EXPAND_KEY = "editor.emmet.expandKey";
export const PREVIEW_KEY = "editor.emmet.html.preview";
export const PAGE_ID = "editor.emmet";

/** The effective `editor.emmet.*` settings with the editor's indent unit. */
export function emmetOptions(view?: EditorView): EmmetOptions {
  return {
    html: getSetting<boolean>("editor.emmet.html.enabled", true),
    css: getSetting<boolean>("editor.emmet.css.enabled", true),
    jsx: getSetting<boolean>("editor.emmet.jsx.enabled", true),
    jsxClassAttr: getSetting<string>("editor.emmet.jsx.classAttribute", "className"),
    hrefRecognition: getSetting<boolean>("editor.emmet.html.hrefAutoDetect", true),
    indent: view ? view.state.facet(indentUnit) : "  ",
  };
}

/** Document, caret and language for a request about `view`. */
export function expandRequest(path: string, view: EditorView, preview = false) {
  return {
    text: view.state.doc.toString(),
    offset: view.state.selection.main.head,
    languageId: languageIdFor(path),
    options: emmetOptions(view),
    preview,
  };
}

const MARKUP = new Set(["html", "xhtml", "vue", "blade", "twig", "smarty", "latte", "jinja", "handlebars", "gotemplate", "php"]);
const JSX = new Set(["jsx", "tsx", "javascript"]);
const CSS = new Set(["css", "scss", "less", "sass", "stylus"]);

/** Emmet's family for a file type: markup (HTML-like), JSX or CSS. */
export function emmetFamily(languageId: string | undefined): "html" | "jsx" | "css" | null {
  if (!languageId) return null;
  if (MARKUP.has(languageId)) return "html";
  if (JSX.has(languageId)) return "jsx";
  if (CSS.has(languageId)) return "css";
  return null;
}

/** Whether markup actions (Surround with Emmet, Update/Remove Tag) apply to the file. */
export function markupEnabled(languageId: string | undefined): boolean {
  const family = emmetFamily(languageId);
  if (family === "html") return getSetting<boolean>("editor.emmet.html.enabled", true);
  if (family === "jsx") return getSetting<boolean>("editor.emmet.jsx.enabled", true);
  return false;
}

/** Insert an expansion; its tab stops become snippet fields. */
export function applyExpansion(view: EditorView, e: EmmetExpansion): void {
  snippet(toSnippet({ ...e, templateId: "emmet" }))(view, null, e.from, e.to);
  view.focus();
}

/** Apply edits computed in Rust as one undo step. */
export function applyEdits(view: EditorView, edits: EmmetEdit[]): void {
  if (!edits.length) return;
  const head = view.state.selection.main.head;
  const changes = view.state.changes(edits.map((e) => ({ from: e.from, to: e.to, insert: e.text })));
  view.dispatch({
    changes,
    selection: EditorSelection.cursor(changes.mapPos(head, -1)),
    userEvent: "input.emmet",
    scrollIntoView: true,
  });
  view.focus();
}
