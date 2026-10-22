/**
 * Debugger in the editor (CodeMirror):
 * - breakpoints drawn over the line numbers (New UI): click toggles, right-click edits;
 * - execution point highlight and the selected frame's line;
 * - inline variable values after the code while suspended.
 */
import { createEffect, createRoot } from "solid-js";
import { RangeSet, StateEffect, StateField, type Extension, type Range } from "@codemirror/state";
import { Decoration, EditorView, GutterMarker, ViewPlugin, WidgetType, lineNumberMarkers, type DecorationSet } from "@codemirror/view";
import { getSetting } from "@/state/settings";
import type { Breakpoint } from "./api";
import { inlineValues } from "./format";
import { addLogpoint, breakpoints, breakpointsMuted, frameIndex, frames, pausedSession, toggleLineBreakpoint, variables } from "./store";
import { openBreakpointPopup } from "./overlays";

export interface EditorDebugState {
  bps: Breakpoint[];
  muted: boolean;
  /** Execution point line (top frame) in this file. */
  exec: number | null;
  /** Line of the selected (non-top) frame in this file. */
  frame: number | null;
  inline: Map<number, string>;
}

const empty: EditorDebugState = { bps: [], muted: false, exec: null, frame: null, inline: new Map() };
const setDebugState = StateEffect.define<EditorDebugState>();

export type BreakpointLook = "enabled" | "disabled" | "conditional" | "muted" | "log";

export function breakpointLook(bp: Breakpoint, muted: boolean): BreakpointLook {
  if (!bp.enabled) return "disabled";
  if (muted) return "muted";
  if (bp.suspend === false) return "log";
  return bp.condition?.trim() ? "conditional" : "enabled";
}

class BreakpointMarker extends GutterMarker {
  constructor(
    readonly look: BreakpointLook | null,
    readonly exec: boolean,
    readonly title: string,
    readonly temporary = false,
  ) {
    super();
  }
  eq(o: BreakpointMarker) {
    return o.look === this.look && o.exec === this.exec && o.title === this.title && o.temporary === this.temporary;
  }
  toDOM() {
    const el = document.createElement("div");
    el.className = "cm-bp-cell";
    el.title = this.title;
    if (this.look) {
      const dot = document.createElement("span");
      dot.className = `cm-bp-dot cm-bp-${this.look}`;
      if (this.exec) dot.classList.add("cm-bp-hit");
      if (this.temporary) dot.classList.add("cm-bp-temp");
      el.append(dot);
    } else if (this.exec) {
      const arrow = document.createElement("span");
      arrow.className = "cm-exec-arrow";
      el.append(arrow);
    }
    return el;
  }
}

function bpTitle(bp: Breakpoint, look: BreakpointLook): string {
  const where = `Line ${bp.line}${bp.temporary ? " (temporary)" : ""}`;
  if (look === "disabled") return `${where} (disabled)`;
  if (look === "muted") return `${where} (breakpoints are muted)`;
  const parts = [where];
  if (look === "log") parts.push("does not suspend");
  if (bp.condition?.trim()) parts.push(`condition: ${bp.condition}`);
  if (bp.logExpression?.trim()) parts.push(`logs: ${bp.logExpression}`);
  return parts.join(", ");
}

class InlineValues extends WidgetType {
  constructor(readonly text: string) {
    super();
  }
  eq(o: InlineValues) {
    return o.text === this.text;
  }
  toDOM() {
    const el = document.createElement("span");
    el.className = "cm-debug-inline";
    el.textContent = this.text;
    return el;
  }
  ignoreEvent() {
    return true;
  }
}

interface FieldValue {
  state: EditorDebugState;
  markers: RangeSet<GutterMarker>;
  decorations: DecorationSet;
}

function build(view: { doc: { lines: number; line: (n: number) => { from: number; to: number } } }, st: EditorDebugState): FieldValue {
  const doc = view.doc;
  const markers: Range<GutterMarker>[] = [];
  const decos: Range<Decoration>[] = [];
  const byLine = new Map<number, Breakpoint>();
  for (const bp of st.bps) if (bp.line && bp.line <= doc.lines) byLine.set(bp.line, bp);
  const lines = new Set<number>([...byLine.keys()]);
  if (st.exec && st.exec <= doc.lines) lines.add(st.exec);
  for (const l of [...lines].sort((a, b) => a - b)) {
    const bp = byLine.get(l);
    const look = bp ? breakpointLook(bp, st.muted) : null;
    const exec = st.exec === l;
    const title = bp && look ? bpTitle(bp, look) : "Execution point";
    const from = doc.line(l).from;
    markers.push(new BreakpointMarker(look, exec, title, !!bp?.temporary).range(from));
    if (exec) decos.push(Decoration.line({ class: "cm-exec-line" }).range(from));
    else if (look && look !== "disabled" && look !== "log") decos.push(Decoration.line({ class: `cm-bp-line cm-bp-line-${look}` }).range(from));
  }
  if (st.frame && st.frame <= doc.lines && st.frame !== st.exec) {
    decos.push(Decoration.line({ class: "cm-frame-line" }).range(doc.line(st.frame).from));
  }
  for (const [l, text] of st.inline) {
    if (l > doc.lines) continue;
    decos.push(Decoration.widget({ widget: new InlineValues(text), side: 1 }).range(doc.line(l).to));
  }
  return {
    state: st,
    markers: RangeSet.of(markers, true),
    decorations: Decoration.set(decos, true),
  };
}

const debugField = StateField.define<FieldValue>({
  create: (state) => build(state, empty),
  update(value, tr) {
    for (const e of tr.effects) if (e.is(setDebugState)) return build(tr.state, e.value);
    if (!tr.docChanged) return value;
    // Keep markers on their lines while typing; the store resyncs on the next change.
    return { ...value, markers: value.markers.map(tr.changes), decorations: value.decorations.map(tr.changes) };
  },
  provide: (f) => [
    EditorView.decorations.from(f, (v) => v.decorations),
    lineNumberMarkers.from(f, (v) => v.markers),
  ],
});

/** Debugger state of one file, derived from the store. */
export function stateFor(path: string, doc: string[]): EditorDebugState {
  const bps = breakpoints().filter((b) => b.kind === "line" && b.file === path);
  const fs = frames();
  const session = pausedSession();
  const top = fs[0];
  const loc = session?.location;
  const execFile = top?.file ?? loc?.file;
  const execLine = top?.line ?? loc?.line ?? null;
  const exec = session && execFile === path ? execLine : null;
  const selected = fs[frameIndex()];
  const frame = session && selected && frameIndex() > 0 && selected.file === path ? selected.line : null;
  const valuesLine = frameIndex() > 0 ? frame : exec;
  const inline =
    session && valuesLine && getSetting<boolean>("php.debug.showInlineValues", true)
      ? inlineValues(doc, valuesLine, variables())
      : new Map<number, string>();
  return { bps, muted: breakpointsMuted(), exec, frame, inline };
}

function lineAtEvent(view: EditorView, e: MouseEvent): number | null {
  const target = e.target as HTMLElement;
  if (!target.closest(".cm-lineNumbers .cm-gutterElement")) return null;
  const block = view.lineBlockAtHeight(e.clientY - view.documentTop);
  return view.state.doc.lineAt(block.from).number;
}

function controller(path: string) {
  return ViewPlugin.define((view) => {
    let disposed = false;
    const dispose = createRoot((dispose) => {
      createEffect(() => {
        const st = stateFor(path, view.state.doc.toString().split("\n"));
        queueMicrotask(() => {
          if (!disposed) view.dispatch({ effects: setDebugState.of(st) });
        });
      });
      return dispose;
    });
    const onDown = (e: MouseEvent) => {
      if (e.button !== 0) return;
      const line = lineAtEvent(view, e);
      if (line == null) return;
      e.preventDefault();
      if (e.shiftKey) {
        // Shift+click: a breakpoint that logs instead of suspending, with its popup open.
        const { clientX, clientY } = e;
        void addLogpoint(path, line).then((bp) => bp && openBreakpointPopup(bp, clientX, clientY));
        return;
      }
      void toggleLineBreakpoint(path, line);
    };
    const onMenu = (e: MouseEvent) => {
      const line = lineAtEvent(view, e);
      if (line == null) return;
      const bp = breakpoints().find((b) => b.kind === "line" && b.file === path && b.line === line);
      if (!bp) return;
      // Capture phase: handled here instead of the generic gutter menu.
      e.preventDefault();
      e.stopPropagation();
      openBreakpointPopup(bp, e.clientX, e.clientY);
    };
    view.dom.addEventListener("mousedown", onDown);
    view.dom.addEventListener("contextmenu", onMenu, true);
    return {
      destroy() {
        disposed = true;
        dispose();
        view.dom.removeEventListener("mousedown", onDown);
        view.dom.removeEventListener("contextmenu", onMenu, true);
      },
    };
  });
}

const debugTheme = EditorView.baseTheme({
  ".cm-debuggable .cm-lineNumbers .cm-gutterElement": { position: "relative", cursor: "pointer" },
  ".cm-debuggable .cm-lineNumbers .cm-gutterElement:hover::after": {
    content: '""',
    position: "absolute",
    top: "50%",
    left: "50%",
    width: "12px",
    height: "12px",
    transform: "translate(-30%, -50%)",
    borderRadius: "50%",
    background: "var(--danger)",
    opacity: "0.28",
    pointerEvents: "none",
  },
  ".cm-bp-cell": { display: "flex", alignItems: "center", justifyContent: "center", height: "100%", paddingLeft: "6px" },
  ".cm-bp-dot": {
    display: "inline-block",
    width: "12px",
    height: "12px",
    borderRadius: "50%",
    background: "var(--danger)",
    boxSizing: "border-box",
  },
  ".cm-bp-conditional": { position: "relative" },
  ".cm-bp-conditional::after": {
    content: '"?"',
    position: "absolute",
    inset: "0",
    fontSize: "9px",
    fontWeight: "700",
    lineHeight: "12px",
    textAlign: "center",
    color: "var(--bg-editor)",
    fontFamily: "var(--font-ui)",
  },
  ".cm-bp-disabled": { background: "transparent", border: "2px solid var(--danger)" },
  ".cm-bp-muted": { background: "var(--fg-faint)" },
  ".cm-bp-log": { background: "var(--warning)" },
  ".cm-bp-temp": { boxShadow: "0 0 0 1.5px var(--bg-editor), 0 0 0 3px var(--fg-muted)" },
  ".cm-bp-hit": { boxShadow: "0 0 0 2px var(--bg-editor), 0 0 0 3.5px var(--info)" },
  ".cm-exec-arrow": {
    display: "inline-block",
    width: "0",
    height: "0",
    borderTop: "6px solid transparent",
    borderBottom: "6px solid transparent",
    borderLeft: "10px solid var(--info)",
  },
  ".cm-bp-line-enabled, .cm-bp-line-conditional": { background: "color-mix(in srgb, var(--danger) 16%, transparent)" },
  ".cm-exec-line, .cm-exec-line.cm-activeLine": { background: "color-mix(in srgb, var(--info) 24%, transparent)" },
  ".cm-frame-line": { background: "var(--bg-hover)" },
  ".cm-debug-inline": {
    marginLeft: "2.5em",
    color: "var(--fg-muted)",
    fontStyle: "italic",
    fontFamily: "var(--font-ui)",
    fontSize: "var(--fs-small)",
    whiteSpace: "pre",
  },
});

const isPhp = (path: string) => /\.(php|phtml|inc)$/i.test(path);

/** Editor extension for PHP files. */
export function debugEditorExtensions(path: string): Extension | null {
  if (!isPhp(path)) return null;
  return [debugField, controller(path), debugTheme, EditorView.editorAttributes.of({ class: "cm-debuggable" })];
}
