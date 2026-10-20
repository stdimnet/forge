import { EditorView } from "@codemirror/view";
import { HighlightStyle, syntaxHighlighting } from "@codemirror/language";
import { tags as t } from "@lezer/highlight";

/** Editor chrome built on the theme CSS variables, so it follows dark/light switching. */
export const forgeEditorTheme = EditorView.theme({
  "&": { height: "100%", backgroundColor: "var(--bg-editor)", color: "var(--fg)", fontSize: "var(--fs-editor)" },
  ".cm-scroller": {
    fontFamily: "var(--font-mono)",
    lineHeight: "var(--editor-line-height, 1.6)",
    fontVariantLigatures: "var(--editor-ligatures, normal)",
  },
  ".cm-content": { caretColor: "var(--accent)", padding: "6px 0" },
  ".cm-cursor, .cm-dropCursor": { borderLeftColor: "var(--accent)", borderLeftWidth: "2px" },
  "&.cm-focused .cm-selectionBackground, .cm-selectionBackground, ::selection": {
    backgroundColor: "var(--bg-selection) !important",
  },
  ".cm-activeLine": { backgroundColor: "color-mix(in srgb, var(--bg-hover) 55%, transparent)" },
  ".cm-gutters": {
    backgroundColor: "var(--bg-editor)",
    color: "var(--fg-faint)",
    border: "none",
    paddingRight: "6px",
  },
  ".cm-activeLineGutter": { backgroundColor: "transparent", color: "var(--fg-muted)" },
  ".cm-lineNumbers .cm-gutterElement": { padding: "0 10px 0 14px", minWidth: "44px" },
  ".cm-foldGutter .cm-gutterElement": { color: "var(--fg-faint)" },
  ".cm-matchingBracket": { backgroundColor: "var(--accent-soft)", outline: "1px solid var(--accent)" },
  ".cm-searchMatch": { backgroundColor: "rgba(229, 180, 73, 0.25)" },
  ".cm-tooltip": {
    backgroundColor: "var(--bg-elevated)",
    border: "1px solid var(--border-strong)",
    borderRadius: "var(--radius)",
    boxShadow: "var(--shadow)",
    color: "var(--fg)",
  },
  ".cm-tooltip-autocomplete > ul > li[aria-selected]": { backgroundColor: "var(--accent-soft)", color: "var(--fg-strong)" },
  ".cm-panels": { backgroundColor: "var(--bg-panel)", color: "var(--fg)", borderColor: "var(--border)" },
});

export const forgeHighlight = syntaxHighlighting(
  HighlightStyle.define([
    { tag: [t.keyword, t.modifier, t.controlKeyword, t.operatorKeyword, t.definitionKeyword], color: "var(--syn-keyword)" },
    { tag: [t.string, t.special(t.string)], color: "var(--syn-string)" },
    { tag: [t.number, t.bool, t.null, t.atom], color: "var(--syn-number)" },
    { tag: [t.comment, t.lineComment, t.blockComment, t.docComment], color: "var(--syn-comment)", fontStyle: "italic" },
    { tag: [t.variableName, t.special(t.variableName)], color: "var(--syn-variable)" },
    { tag: [t.function(t.variableName), t.function(t.propertyName)], color: "var(--syn-function)" },
    { tag: [t.className, t.typeName, t.namespace], color: "var(--syn-type)" },
    { tag: [t.propertyName, t.attributeName], color: "var(--syn-attribute)" },
    { tag: [t.meta, t.annotation], color: "var(--syn-attribute)" },
    { tag: [t.tagName], color: "var(--syn-tag)" },
    { tag: [t.operator, t.punctuation, t.bracket], color: "var(--syn-operator)" },
    { tag: [t.definition(t.propertyName), t.labelName], color: "var(--syn-keyword)" },
    { tag: [t.attributeValue, t.url], color: "var(--syn-string)" },
    { tag: [t.unit, t.color], color: "var(--syn-number)" },
    { tag: t.heading, color: "var(--syn-keyword)", fontWeight: "700" },
    { tag: t.strong, fontWeight: "700" },
    { tag: t.emphasis, fontStyle: "italic" },
    { tag: t.strikethrough, textDecoration: "line-through" },
    { tag: t.link, color: "var(--syn-type)", textDecoration: "underline" },
    { tag: t.monospace, color: "var(--syn-string)" },
    { tag: t.quote, color: "var(--syn-comment)" },
    { tag: t.inserted, color: "var(--vcs-added)" },
    { tag: t.deleted, color: "var(--danger)" },
    { tag: t.changed, color: "var(--vcs-modified)" },
    { tag: t.processingInstruction, color: "var(--syn-keyword)" },
    { tag: t.invalid, color: "var(--danger)" },
  ]),
);
