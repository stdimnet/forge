/**
 * The banner above an editor whose file has unresolved merge conflicts (PHPStorm's yellow
 * "File has merge conflicts" bar) with a link to the merge window. It follows `git status`.
 */
import { createEffect, createRoot } from "solid-js";
import { showPanel, type Panel } from "@codemirror/view";
import type { Extension } from "@codemirror/state";
import { TriangleAlert } from "lucide-solid";
import { h, iconNode } from "@/features/lsp/util";
import { hasConflict, openMerge } from "./conflicts";

export function conflictBanner(path: string): Extension {
  return showPanel.of((): Panel => {
    const dom = h("div", "merge-banner");
    dom.hidden = true;
    const resolve = h("button", "link-btn merge-banner-link", "Resolve…");
    resolve.onclick = () => void openMerge(path);
    dom.append(iconNode(TriangleAlert, 14, "merge-banner-icon"), h("span", "merge-banner-text", "File has merge conflicts"), resolve);
    let dispose = () => {};
    createRoot((d) => {
      dispose = d;
      createEffect(() => (dom.hidden = !hasConflict(path)));
    });
    return { dom, top: true, destroy: () => dispose() };
  });
}
