/** Quick documentation on mouse hover, combined with the problems under the mouse. */
import type { Extension, Text } from "@codemirror/state";
import { hoverTooltip, type EditorView } from "@codemirror/view";
import { lspApi, type EditorHover } from "@/ipc/lsp";
import { diagnosticsAt } from "./diagnostics";
import { buildHoverDom, hoverDocOf } from "./hoverDoc";
import { syncNow } from "./sync";
import { fromLsp, toLsp } from "./util";

/** The last hover request, shared so other hover sources can ask without a second request. */
let last: { path: string; pos: number; doc: Text; result: Promise<EditorHover | null> } | null = null;

/** The language server's hover at `pos` (one request per position and document version). */
export function hoverAt(path: string, view: EditorView, pos: number): Promise<EditorHover | null> {
  const doc = view.state.doc;
  if (last && last.path === path && last.pos === pos && last.doc === doc) return last.result;
  const result = (async () => {
    await syncNow(path);
    return lspApi.hover(path, toLsp(doc, pos)).catch(() => null);
  })();
  last = { path, pos, doc, result };
  return result;
}

const atLineEdge = (view: EditorView, pos: number, side: -1 | 1) => {
  const line = view.state.doc.lineAt(pos);
  return (side < 0 && pos === line.from) || (side > 0 && pos === line.to);
};

/**
 * Whether the LSP hover shows something at `pos` (documentation or problems). Other hover
 * sources (e.g. the PHP index) stay quiet then, so only one tooltip appears.
 */
export async function lspClaimsHover(path: string, view: EditorView, pos: number): Promise<boolean> {
  if (diagnosticsAt(view.state, pos).length) return true;
  const hover = await hoverAt(path, view, pos);
  return !!hover?.contents.trim();
}

export function lspHover(path: string): Extension {
  return hoverTooltip(
    async (view, pos, side) => {
      // Ignore hovers past the end of a line.
      if (atLineEdge(view, pos, side)) return null;
      const problems = diagnosticsAt(view.state, pos);
      const hover = await hoverAt(path, view, pos);
      if (!hover?.contents.trim() && !problems.length) return null;
      const doc = await hoverDocOf(hover?.contents);
      const word = view.state.wordAt(pos);
      const from = hover?.range ? fromLsp(view.state.doc, hover.range.start) : (word?.from ?? pos);
      const to = hover?.range ? fromLsp(view.state.doc, hover.range.end) : (word?.to ?? pos);
      return {
        pos: from,
        end: to,
        above: false,
        create: () => ({ dom: buildHoverDom(doc, problems) }),
      };
    },
    { hoverTime: 450 },
  );
}
