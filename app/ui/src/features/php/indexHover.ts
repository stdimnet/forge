/**
 * Index-based hover fallback for PHP files: shows signature, attributes (e.g. #[Route])
 * and the parsed docblock of project symbols.
 *
 * Deliberately separate from the generic LSP hover: an LSP hover provider registers itself
 * with `setOtherHoverProvider`, and this source then only answers where it had nothing.
 */
import { hoverTooltip, type Tooltip } from "@codemirror/view";
import type { Extension } from "@codemirror/state";
import { phpApi } from "./api";
import { renderMarkdown } from "./markdown";

type OtherHover = (path: string, offset: number) => Promise<boolean> | boolean;

let otherHover: OtherHover | null = null;

/** Let another hover provider (LSP) claim positions first. */
export function setOtherHoverProvider(fn: OtherHover | null): void {
  otherHover = fn;
}

export function indexHover(path: string): Extension {
  return hoverTooltip(
    async (view, pos): Promise<Tooltip | null> => {
      if (otherHover && (await otherHover(path, pos))) return null;
      let hover;
      try {
        hover = await phpApi.hover(path, pos, view.state.doc.toString());
      } catch {
        return null;
      }
      if (!hover) return null;
      return {
        pos: hover.start,
        end: hover.end,
        above: true,
        create: () => {
          const dom = document.createElement("div");
          dom.className = "php-hover";
          dom.innerHTML = renderMarkdown(hover.markdown);
          return { dom };
        },
      };
    },
    { hoverTime: 350 },
  );
}
