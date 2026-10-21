/**
 * Sticky Lines (PHPStorm 2023.3+): while scrolling, the headers of the scopes that enclose
 * the top of the editor (classes, functions, `if` / `for` blocks, CSS rules, HTML parents,
 * Lua functions) stay at the top, with their line numbers and syntax colors. Clicking one
 * scrolls to it; the context menu configures them. The scopes come from Rust
 * (`model.ts`); this module only measures and draws.
 */
import { createEffect, createRoot, on } from "solid-js";
import type { EditorState } from "@codemirror/state";
import { EditorView, ViewPlugin, type PluginValue, type ViewUpdate } from "@codemirror/view";
import { syntaxTree } from "@codemirror/language";
import { highlightTree } from "@lezer/highlight";
import { forgeHighlightStyle } from "@/editor/forgeTheme";
import { openMenu } from "@/features/run/PopupMenu";
import { openSettings } from "@/features/settings/dialogState";
import { getSetting, saveSetting } from "@/state/settings";
import {
  STICKY_DISABLED,
  languageLabelOf,
  maxStickyLines,
  stickyLinesOn,
  structureOf,
  structureReady,
  type Scope,
} from "./model";

/** The sticky scopes for a scroll position: those whose header is above the row it would take. */
export function stickyScopes(scopes: Scope[], lineAt: (row: number) => number, max: number): Scope[] {
  let rows: Scope[] = [];
  for (let attempt = 0; attempt <= max + 1; attempt++) {
    const line = lineAt(rows.length);
    const next = scopes.filter((s) => s.sticky && s.line < line && s.endLine > line).slice(0, max);
    if (next.length === rows.length && next.every((s, i) => s === rows[i])) break;
    // Fewer rows uncover lines whose scopes may have ended: keep the smaller set.
    if (attempt > 0 && next.length < rows.length) {
      rows = next;
      break;
    }
    rows = next;
  }
  return rows;
}

/** The syntax-colored text of a line, as the editor shows it. */
function highlightedLine(state: EditorState, lineNo: number): DocumentFragment {
  const line = state.doc.line(lineNo + 1);
  const frag = document.createDocumentFragment();
  let pos = line.from;
  const plain = (to: number) => {
    if (to > pos) frag.append(state.sliceDoc(pos, to));
  };
  highlightTree(
    syntaxTree(state),
    forgeHighlightStyle,
    (from, to, classes) => {
      plain(from);
      const span = document.createElement("span");
      span.className = classes;
      span.textContent = state.sliceDoc(from, to);
      frag.append(span);
      pos = to;
    },
    line.from,
    line.to,
  );
  plain(line.to);
  return frag;
}

interface Measured {
  rows: Scope[];
  top: number;
  width: number;
  gutter: number;
  /** Space right of the line numbers inside the gutters. */
  numberRight: number;
  textLeft: number;
  lineHeight: number;
}

class StickyLines implements PluginValue {
  readonly dom: HTMLDivElement;
  private shown = "";
  private readonly dispose: () => void;
  private readonly onScroll = () => this.schedule();

  constructor(
    readonly view: EditorView,
    readonly language: string,
  ) {
    this.dom = document.createElement("div");
    this.dom.className = "cm-sticky-lines";
    this.dom.setAttribute("aria-hidden", "true");
    this.dom.addEventListener("contextmenu", (e) => this.menu(e));
    view.dom.appendChild(this.dom);
    view.scrollDOM.addEventListener("scroll", this.onScroll, { passive: true });
    this.dispose = createRoot((d) => {
      createEffect(on([() => stickyLinesOn(language), maxStickyLines, structureReady], () => this.schedule()));
      return d;
    });
  }

  update(u: ViewUpdate): void {
    if (u.docChanged || u.viewportChanged || u.geometryChanged || u.heightChanged) this.schedule();
  }

  schedule(): void {
    this.view.requestMeasure({ key: this, read: (v) => this.measure(v), write: (m) => this.draw(m) });
  }

  private measure(view: EditorView): Measured | null {
    if (!stickyLinesOn(this.language)) return null;
    const scopes = structureOf(view.state.doc, this.language).scopes;
    if (!scopes.length) return null;
    const scroller = view.scrollDOM.getBoundingClientRect();
    const editor = view.dom.getBoundingClientRect();
    // Document height at the top edge of the scroller.
    const top = scroller.top - view.documentTop;
    if (top <= 0) return null;
    const lineHeight = view.defaultLineHeight;
    const rows = stickyScopes(
      scopes,
      (row) => view.state.doc.lineAt(view.lineBlockAtHeight(top + row * lineHeight + 1).from).number - 1,
      maxStickyLines(),
    );
    const gutters = view.dom.querySelector(".cm-gutters");
    const numbers = view.dom.querySelector(".cm-lineNumbers .cm-gutterElement:not([style*='visibility'])");
    const firstLine = view.contentDOM.querySelector(".cm-line");
    const padding = firstLine ? Number.parseFloat(getComputedStyle(firstLine).paddingLeft) || 0 : 0;
    return {
      rows,
      top: scroller.top - editor.top,
      width: view.scrollDOM.clientWidth,
      gutter: gutters ? gutters.getBoundingClientRect().width : 0,
      numberRight: numbers && gutters ? gutters.getBoundingClientRect().right - numbers.getBoundingClientRect().right + 10 : 28,
      textLeft: view.contentDOM.getBoundingClientRect().left - editor.left + padding,
      lineHeight,
    };
  }

  private draw(m: Measured | null): void {
    if (!m || !m.rows.length) {
      this.dom.style.display = "none";
      this.dom.replaceChildren();
      this.shown = "";
      return;
    }
    const s = this.dom.style;
    s.display = "block";
    s.top = `${m.top}px`;
    s.width = `${m.width}px`;
    s.setProperty("--sticky-gutter", `${m.gutter}px`);
    s.setProperty("--sticky-number-right", `${m.numberRight}px`);
    s.setProperty("--sticky-text-left", `${m.textLeft - m.gutter}px`);
    s.setProperty("--sticky-line-height", `${m.lineHeight}px`);
    s.tabSize = this.view.contentDOM.style.tabSize;
    const key = m.rows.map((r) => `${r.line}:${r.endLine}`).join(",");
    if (key === this.shown && this.dom.childElementCount === m.rows.length) return;
    this.shown = key;
    const state = this.view.state;
    const showNumbers = getSetting<boolean>("editor.showLineNumbers", true) !== false;
    this.dom.replaceChildren(
      ...m.rows.map((scope) => {
        const row = document.createElement("div");
        row.className = "cm-sticky-line";
        row.title = scope.name;
        const gutter = document.createElement("div");
        gutter.className = "cm-sticky-gutter";
        if (showNumbers) gutter.textContent = String(scope.line + 1);
        const text = document.createElement("div");
        text.className = "cm-sticky-text";
        const inner = document.createElement("span");
        inner.className = "cm-sticky-code";
        if (scope.line < state.doc.lines) inner.append(highlightedLine(state, scope.line));
        text.append(inner);
        row.append(gutter, text);
        row.addEventListener("mousedown", (e) => {
          if (e.button !== 0) return;
          e.preventDefault();
          this.jump(scope);
        });
        return row;
      }),
    );
  }

  private jump(scope: Scope): void {
    const doc = this.view.state.doc;
    if (scope.line >= doc.lines) return;
    const line = doc.line(scope.line + 1);
    const pos = Math.min(line.from + scope.column, line.to);
    this.view.dispatch({ selection: { anchor: pos }, effects: EditorView.scrollIntoView(pos, { y: "start" }) });
    this.view.focus();
  }

  private menu(e: MouseEvent): void {
    e.preventDefault();
    const label = languageLabelOf(this.language);
    openMenu({
      x: e.clientX,
      y: e.clientY,
      items: [
        { label: "Configure Sticky Lines…", run: () => openSettings("editor.general") },
        {
          label: `Disable Sticky Lines for ${label}`,
          run: () => {
            const off = getSetting<string[]>(STICKY_DISABLED, []);
            if (!off.includes(this.language)) void saveSetting(STICKY_DISABLED, [...off, this.language]);
          },
        },
      ],
    });
  }

  destroy(): void {
    this.dispose();
    this.view.scrollDOM.removeEventListener("scroll", this.onScroll);
    this.dom.remove();
  }
}

const stickyTheme = EditorView.baseTheme({
  ".cm-sticky-lines": {
    position: "absolute",
    left: "0",
    zIndex: "5",
    display: "none",
    overflow: "hidden",
    backgroundColor: "var(--bg-editor)",
    borderBottom: "1px solid var(--border-strong)",
    fontFamily: "var(--font-mono)",
    cursor: "pointer",
  },
  ".cm-sticky-line": {
    position: "relative",
    display: "flex",
    height: "var(--sticky-line-height)",
    lineHeight: "var(--sticky-line-height)",
    whiteSpace: "pre",
  },
  ".cm-sticky-line:hover": { backgroundColor: "var(--bg-hover)" },
  ".cm-sticky-gutter": {
    flex: "0 0 var(--sticky-gutter)",
    boxSizing: "border-box",
    paddingRight: "var(--sticky-number-right)",
    textAlign: "right",
    color: "var(--fg-faint)",
    backgroundColor: "inherit",
  },
  ".cm-sticky-text": { position: "relative", flex: "1", overflow: "hidden" },
  ".cm-sticky-code": { position: "absolute", left: "var(--sticky-text-left)", color: "var(--fg)" },
});

/** The Sticky Lines extension of an editor for `language` (a file type id). */
export function stickyLines(language: string) {
  return [ViewPlugin.define((view) => new StickyLines(view, language)), stickyTheme];
}
