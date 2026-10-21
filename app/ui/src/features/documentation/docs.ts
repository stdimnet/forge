/**
 * Documentation of the symbol at a position: the language servers' hover first, the PHP
 * index's hover for PHP files without one. Shared by the Quick Documentation popup and the
 * Documentation tool window.
 */
import type { EditorView } from "@codemirror/view";
import { hoverAt } from "@/features/lsp/hover";
import { buildHoverDom, hoverDocOf } from "@/features/lsp/hoverDoc";
import { renderMarkdown } from "@/features/lsp/markdown";
import { h } from "@/features/lsp/util";
import { phpApi } from "@/features/php/api";

export interface Documentation {
  /** The file and position the documentation is for (External Documentation, Edit Source). */
  path: string;
  pos: number;
  /** The name at the position, for titles. */
  name: string;
  /** Builds a fresh DOM each time (the popup and the tool window both show it). */
  render: () => HTMLElement;
}

const isPhp = (path: string) => /\.(php|phtml|inc)$/i.test(path) && !path.endsWith(".blade.php");

/** The code language of a file's signatures for syntax colors. */
export const signatureLanguage = (path: string) => (/\.lua$/i.test(path) ? "lua" : /\.(php|phtml|inc)$/i.test(path) ? "php" : "text");

function nameAt(view: EditorView, pos: number): string {
  const word = view.state.wordAt(pos) ?? (pos > 0 ? view.state.wordAt(pos - 1) : null);
  return word ? view.state.sliceDoc(word.from, word.to) : "";
}

/** The documentation at `pos`, or null when there is none. */
export async function documentationAt(path: string, view: EditorView, pos: number): Promise<Documentation | null> {
  const name = nameAt(view, pos);
  const hover = await hoverAt(path, view, pos);
  if (hover?.contents.trim()) {
    const doc = await hoverDocOf(hover.contents);
    const lang = signatureLanguage(path);
    return { path, pos, name, render: () => buildHoverDom(doc, [], lang) };
  }
  if (!isPhp(path)) return null;
  const indexed = await phpApi.hover(path, pos, view.state.doc.toString()).catch(() => null);
  if (!indexed?.markdown.trim()) return null;
  return { path, pos, name, render: () => h("div", "doc-index", renderMarkdown(indexed.markdown)) };
}
