/**
 * Markdown of language server documentation: parsed into a safe AST by Rust
 * (`forge-markdown`, flavor `docs`) and rendered with the Solid components of
 * `MarkdownDoc.tsx`. Raw HTML never reaches the DOM.
 */
import { markdownApi } from "@/ipc/markdown";
import { appendBlocks } from "./MarkdownDoc";

export { highlightInto } from "./highlightCode";

/**
 * Element that shows `md` once the backend has parsed it, for synchronous callers (hover
 * popups of the language modes). It does not take part in layout (`display: contents`).
 */
export function renderMarkdown(md: string): HTMLElement {
  const el = document.createElement("div");
  el.style.display = "contents";
  void markdownApi
    .parse(md, "docs")
    .then((blocks) => appendBlocks(el, blocks))
    .catch(() => el.append(md));
  return el;
}
