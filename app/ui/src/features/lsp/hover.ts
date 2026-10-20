/** Quick documentation on mouse hover, combined with the problems under the mouse. */
import type { Extension } from "@codemirror/state";
import { hoverTooltip } from "@codemirror/view";
import { lspApi } from "@/ipc/lsp";
import { diagnosticsAt } from "./diagnostics";
import { buildHoverDom } from "./hoverDoc";
import { syncNow } from "./sync";
import { fromLsp, toLsp } from "./util";

export function lspHover(path: string): Extension {
  return hoverTooltip(
    async (view, pos, side) => {
      const line = view.state.doc.lineAt(pos);
      // Ignore hovers past the end of a line.
      if ((side < 0 && pos === line.from) || (side > 0 && pos === line.to)) return null;
      const problems = diagnosticsAt(view.state, pos);
      await syncNow(path);
      const hover = await lspApi.hover(path, toLsp(view.state.doc, pos)).catch(() => null);
      if (!hover?.contents.trim() && !problems.length) return null;
      const word = view.state.wordAt(pos);
      const from = hover?.range ? fromLsp(view.state.doc, hover.range.start) : (word?.from ?? pos);
      const to = hover?.range ? fromLsp(view.state.doc, hover.range.end) : (word?.to ?? pos);
      return {
        pos: from,
        end: to,
        above: false,
        create: () => ({ dom: buildHoverDom(hover?.contents, problems) }),
      };
    },
    { hoverTime: 450 },
  );
}
