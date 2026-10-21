/**
 * Custom folding regions, like PHPStorm: `// region Name` … `// endregion`, `#region`,
 * `<!-- region -->`, `/* region *\/`, `-- region` and NetBeans' `// <editor-fold
 * desc="Name">`. A region folds from its start comment to the end comment and shows its
 * name as the placeholder; `defaultstate="collapsed"` regions start folded. Surround With
 * (Ctrl+Alt+T) › "region...endregion Comments" creates them. The regions come from Rust.
 */
import type { EditorState, Extension } from "@codemirror/state";
import { EditorView, ViewPlugin } from "@codemirror/view";
import { codeFolding, foldEffect, foldService, foldedRanges } from "@codemirror/language";
import { structureOf, type Region } from "./model";

/** The document range a region folds. */
export function regionRange(state: EditorState, r: Region): { from: number; to: number } | null {
  const doc = state.doc;
  if (r.endLine >= doc.lines) return null;
  const start = doc.line(r.line + 1);
  const end = doc.line(r.endLine + 1);
  return { from: Math.min(start.from + r.column, start.to), to: Math.min(end.from + r.endColumn, end.to) };
}

function regionStartingAt(state: EditorState, language: string, pos: number): Region | undefined {
  const line = state.doc.lineAt(pos).number - 1;
  return structureOf(state.doc, language).regions.find((r) => r.line === line);
}

function placeholder(onclick: (event: Event) => void, text: string, region: boolean): HTMLElement {
  const el = document.createElement("span");
  el.textContent = text;
  el.className = region ? "cm-foldPlaceholder cm-region-placeholder" : "cm-foldPlaceholder";
  el.setAttribute("aria-label", "folded code");
  el.title = "unfold";
  el.onclick = onclick;
  return el;
}

const regionTheme = EditorView.baseTheme({
  ".cm-region-placeholder": {
    fontFamily: "var(--font-ui)",
    fontSize: "0.9em",
    padding: "0 6px",
    color: "var(--fg-muted)",
  },
});

/** Region folding for an editor of `language`. */
export function regionFolding(language: string): Extension {
  return [
    foldService.of((state, lineStart) => {
      const region = regionStartingAt(state, language, lineStart);
      return region ? regionRange(state, region) : null;
    }),
    codeFolding({
      preparePlaceholder: (state, range) => {
        const region = regionStartingAt(state, language, range.from);
        const r = region && regionRange(state, region);
        return r && r.from === range.from ? region.name || "..." : null;
      },
      placeholderDOM: (_view, onclick, prepared: string | null) => placeholder(onclick, prepared ?? "…", prepared != null),
    }),
    regionTheme,
    // `<editor-fold defaultstate="collapsed">` starts folded.
    ViewPlugin.define((view) => {
      queueMicrotask(() => {
        if (!view.dom.isConnected && !view.dom.parentNode) return;
        const folded = foldedRanges(view.state);
        const effects = structureOf(view.state.doc, language)
          .regions.filter((r) => r.collapsed)
          .map((r) => regionRange(view.state, r))
          .filter((r): r is { from: number; to: number } => {
            if (!r) return false;
            let already = false;
            folded.between(r.from, r.to, (from, to) => {
              if (from === r.from && to === r.to) already = true;
            });
            return !already;
          })
          .map((r) => foldEffect.of(r));
        if (effects.length) view.dispatch({ effects });
      });
      return {};
    }),
  ];
}
