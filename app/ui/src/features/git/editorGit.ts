/**
 * Git in the editor (CodeMirror extensions):
 * - change markers in the gutter comparing the buffer with HEAD (added / modified / deleted),
 *   clickable for Rollback and Show Diff;
 * - "Annotate with Git Blame" gutter left of the line numbers, colored by commit age;
 * - inline blame hint at the end of the caret line ("Marlon, 2 days ago • message").
 */
import { createEffect, createRoot, on } from "solid-js";
import { Chunk } from "@codemirror/merge";
import { Compartment, Prec, RangeSet, StateEffect, StateField, Text, type Extension, type Range } from "@codemirror/state";
import {
  Decoration,
  EditorView,
  GutterMarker,
  ViewPlugin,
  WidgetType,
  gutter,
  type DecorationSet,
  type ViewUpdate,
} from "@codemirror/view";
import { normalizeEol, saveFile, setTabEol } from "@/state/editor";
import { gitApi, type Blame, type BlameCommit } from "./api";
import { fileReload, revision, status } from "./store";
import { annotated, setBlameCard, setDiffRequest, toggleAnnotate } from "./overlays";
import { ageBucket, shortDate, timeAgo } from "./format";
import { replaceDocument } from "@/editor/reload";
import { lineDiffConfig, rollbackEdit } from "./lineDiff";

// ---- shared per-view git data ---------------------------------------------------------

interface BlameState {
  commits: BlameCommit[];
  /** Per 0-based line: commit index, or -1 for lines changed since the blame was taken. */
  lines: number[];
  minTime: number;
  maxTime: number;
}

const setBlameEffect = StateEffect.define<Blame | null>();

const blameField = StateField.define<BlameState | null>({
  create: () => null,
  update(value, tr) {
    for (const e of tr.effects) {
      if (e.is(setBlameEffect)) {
        const b = e.value;
        if (!b) return null;
        const times = b.commits.filter((c) => !c.uncommitted).map((c) => c.time);
        return {
          commits: b.commits,
          lines: b.lines.slice(),
          minTime: times.length ? Math.min(...times) : 0,
          maxTime: times.length ? Math.max(...times) : 0,
        };
      }
    }
    if (!value || !tr.docChanged) return value;
    // Keep line → commit aligned while editing: changed lines become "uncommitted".
    const lines = value.lines.slice();
    const changes: [number, number, number][] = [];
    tr.changes.iterChanges((fromA, toA, fromB, toB) => {
      const a0 = tr.startState.doc.lineAt(fromA).number - 1;
      const a1 = tr.startState.doc.lineAt(toA).number - 1;
      const b0 = tr.state.doc.lineAt(fromB).number - 1;
      const b1 = tr.state.doc.lineAt(toB).number - 1;
      changes.push([a0, a1 - a0 + 1, b1 - b0 + 1]);
    });
    for (const [start, removed, added] of changes.reverse()) lines.splice(start, removed, ...new Array<number>(added).fill(-1));
    return { ...value, lines };
  },
});

/** The commit for a 1-based line, or null when uncommitted/unknown. */
function commitAt(state: BlameState | null, line: number): BlameCommit | null {
  if (!state) return null;
  const idx = state.lines[line - 1];
  const c = idx == null || idx < 0 ? null : state.commits[idx];
  return c && !c.uncommitted ? c : null;
}

// ---- change markers -------------------------------------------------------------------

type ChangeKind = "added" | "modified" | "deleted";

class ChangeMarker extends GutterMarker {
  constructor(readonly kind: ChangeKind, readonly chunk: number) {
    super();
  }
  eq(other: ChangeMarker) {
    return other.kind === this.kind && other.chunk === this.chunk;
  }
  toDOM() {
    const el = document.createElement("div");
    el.className = `cm-git-change cm-git-${this.kind}`;
    el.title = this.kind === "deleted" ? "Deleted lines (click for actions)" : `${this.kind === "added" ? "Added" : "Modified"} lines (click for actions)`;
    return el;
  }
}

function markersFor(chunks: readonly Chunk[], doc: Text): RangeSet<GutterMarker> {
  const ranges: Range<GutterMarker>[] = [];
  chunks.forEach((c, i) => {
    if (c.fromB === c.toB) {
      const pos = Math.min(c.fromB, doc.length);
      ranges.push(new ChangeMarker("deleted", i).range(doc.lineAt(pos).from));
      return;
    }
    const kind: ChangeKind = c.fromA === c.toA ? "added" : "modified";
    const first = doc.lineAt(c.fromB).number;
    const last = doc.lineAt(Math.min(c.endB, doc.length)).number;
    for (let l = first; l <= last; l++) ranges.push(new ChangeMarker(kind, i).range(doc.line(l).from));
  });
  return RangeSet.of(ranges, true);
}

// ---- blame annotation gutter ----------------------------------------------------------

class BlameMarker extends GutterMarker {
  constructor(readonly commit: BlameCommit | null, readonly first: boolean, readonly bucket: number) {
    super();
  }
  eq(o: BlameMarker) {
    return o.commit?.sha === this.commit?.sha && o.first === this.first && o.bucket === this.bucket;
  }
  toDOM() {
    const el = document.createElement("div");
    el.className = `cm-git-blame cm-git-age-${this.bucket}`;
    if (this.commit && this.first) {
      const date = document.createElement("span");
      date.className = "cm-git-blame-date";
      date.textContent = shortDate(this.commit.time);
      const author = document.createElement("span");
      author.className = "cm-git-blame-author";
      author.textContent = this.commit.author;
      el.append(date, author);
    } else if (!this.commit) {
      el.classList.add("cm-git-blame-uncommitted");
    }
    return el;
  }
}

class BlameSpacer extends GutterMarker {
  toDOM() {
    const el = document.createElement("div");
    el.className = "cm-git-blame cm-git-blame-spacer";
    el.innerHTML = '<span class="cm-git-blame-date">00.00.00</span><span class="cm-git-blame-author">Jonas Klein</span>';
    return el;
  }
}

function blameGutter(path: string): Extension {
  return Prec.highest(
    gutter({
      class: "cm-git-blame-gutter",
      markers(view) {
        const st = view.state.field(blameField, false) ?? null;
        if (!st) return RangeSet.empty;
        const ranges: Range<GutterMarker>[] = [];
        let prevSha: string | null | undefined;
        for (let l = 1; l <= view.state.doc.lines; l++) {
          const c = commitAt(st, l);
          const sha = c?.sha ?? null;
          const bucket = c ? ageBucket(c.time, st.minTime, st.maxTime) : 0;
          ranges.push(new BlameMarker(c, sha !== prevSha, bucket).range(view.state.doc.line(l).from));
          prevSha = sha;
        }
        return RangeSet.of(ranges);
      },
      initialSpacer: () => new BlameSpacer(),
      domEventHandlers: {
        mouseover(view, block, event) {
          const line = view.state.doc.lineAt(block.from).number;
          const c = commitAt(view.state.field(blameField, false) ?? null, line);
          const target = event.target as HTMLElement;
          if (!c) {
            setBlameCard(null);
            return false;
          }
          const rect = target.closest(".cm-gutterElement")?.getBoundingClientRect() ?? target.getBoundingClientRect();
          setBlameCard({ x: rect.right + 6, y: rect.top, commit: c, path });
          return false;
        },
        mouseleave() {
          setBlameCard(null);
          return false;
        },
      },
    }),
  );
}

// ---- inline blame hint ----------------------------------------------------------------

class InlineBlameWidget extends WidgetType {
  constructor(readonly text: string) {
    super();
  }
  eq(o: InlineBlameWidget) {
    return o.text === this.text;
  }
  toDOM() {
    const el = document.createElement("span");
    el.className = "cm-git-inline-blame";
    el.textContent = this.text;
    return el;
  }
  ignoreEvent() {
    return true;
  }
}

function inlineBlameDecorations(view: EditorView): DecorationSet {
  const st = view.state.field(blameField, false) ?? null;
  if (!st) return Decoration.none;
  const line = view.state.doc.lineAt(view.state.selection.main.head);
  const c = commitAt(st, line.number);
  if (!c) return Decoration.none;
  const text = `${c.author}, ${timeAgo(c.time)} • ${c.summary}`;
  return Decoration.set([Decoration.widget({ widget: new InlineBlameWidget(text), side: 1 }).range(line.to)]);
}

const inlineBlame = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet;
    constructor(view: EditorView) {
      this.decorations = inlineBlameDecorations(view);
    }
    update(u: ViewUpdate) {
      if (u.docChanged || u.selectionSet || u.transactions.some((t) => t.effects.some((e) => e.is(setBlameEffect))))
        this.decorations = inlineBlameDecorations(u.view);
    }
  },
  { decorations: (v) => v.decorations },
);

// ---- controller plugin: fetches HEAD/blame and reacts to app state ---------------------

const setHeadEffect = StateEffect.define<string | null>();

interface ChangeState {
  head: Text | null;
  chunks: readonly Chunk[];
  markers: RangeSet<GutterMarker>;
}

const changeField = StateField.define<ChangeState>({
  create: () => ({ head: null, chunks: [], markers: RangeSet.empty }),
  update(value, tr) {
    let head = value.head;
    let rebuild = false;
    for (const e of tr.effects)
      if (e.is(setHeadEffect)) {
        head = e.value == null ? null : Text.of(normalizeEol(e.value).text.split("\n"));
        rebuild = true;
      }
    if (!head) return rebuild ? { head: null, chunks: [], markers: RangeSet.empty } : value;
    if (!rebuild && !tr.docChanged) return value;
    const chunks = rebuild ? Chunk.build(head, tr.state.doc, lineDiffConfig) : Chunk.updateB(value.chunks, head, tr.state.doc, tr.changes, lineDiffConfig);
    return { head, chunks, markers: markersFor(chunks, tr.state.doc) };
  },
});

function changeGutter(path: string): Extension {
  return gutter({
    class: "cm-git-change-gutter",
    markers: (view) => view.state.field(changeField).markers,
    domEventHandlers: {
      mousedown(view, block, event) {
        const st = view.state.field(changeField);
        const line = view.state.doc.lineAt(block.from);
        const idx = st.chunks.findIndex((c) =>
          c.fromB === c.toB ? view.state.doc.lineAt(Math.min(c.fromB, view.state.doc.length)).number === line.number : c.fromB <= line.from && line.from < Math.max(c.endB, c.fromB + 1),
        );
        if (idx < 0 || !st.head) return false;
        const e = event as MouseEvent;
        showChangePopup(view, path, st.chunks[idx], st.head, { x: e.clientX + 8, y: e.clientY - 6 });
        return true;
      },
    },
  });
}

function showChangePopup(view: EditorView, path: string, chunk: Chunk, head: Text, at: { x: number; y: number }): void {
  document.querySelector(".git-change-popup")?.remove();
  const el = document.createElement("div");
  el.className = "git-change-popup";
  el.style.left = `${at.x}px`;
  el.style.top = `${at.y}px`;
  const removed = head.sliceString(chunk.fromA, Math.min(chunk.toA, head.length));
  const action = (label: string, title: string, run: () => void) => {
    const b = document.createElement("button");
    b.textContent = label;
    b.title = title;
    b.onclick = () => {
      el.remove();
      run();
    };
    el.append(b);
  };
  action("Rollback", "Rollback Lines (Ctrl+Alt+Z)", () => {
    view.dispatch({ changes: rollbackEdit(chunk, head, view.state.doc) });
  });
  action("Show Diff", "Show Diff (Ctrl+D)", () => {
    setDiffRequest({
      path,
      left: { title: "HEAD", content: head.toString() },
      right: { title: "Your version", content: view.state.doc.toString() },
      editable: true,
    });
  });
  if (removed) {
    const pre = document.createElement("pre");
    pre.textContent = removed.replace(/\n$/, "");
    el.append(pre);
  }
  document.body.append(el);
  const close = (e: MouseEvent) => {
    if (!el.contains(e.target as Node)) {
      el.remove();
      document.removeEventListener("mousedown", close, true);
    }
  };
  setTimeout(() => document.addEventListener("mousedown", close, true));
}

/** The lines of each change hunk (0-based, end exclusive; deleted lines: `start == end`). */
export function changeRanges(chunks: readonly Chunk[], doc: Text): { start: number; end: number }[] {
  return chunks.map((c) => {
    const start = doc.lineAt(Math.min(c.fromB, doc.length)).number - 1;
    if (c.fromB === c.toB) return { start, end: start };
    return { start, end: doc.lineAt(Math.min(c.endB, doc.length)).number };
  });
}

/**
 * Next Change / Previous Change (Ctrl+Alt+Shift+Down/Up, PHPStorm's
 * `VcsShowNextChangeMarker`): move the caret to the next or previous change hunk (the order,
 * wrapping around, comes from Rust: `forge_editing::change_target`) and show its popup, as
 * a click on the gutter marker does. `target` picks the hunk index from the ranges.
 */
export function showChangeMarker(
  view: EditorView,
  path: string,
  target: (ranges: { start: number; end: number }[], line: number) => number | null,
): boolean {
  const st = view.state.field(changeField, false);
  if (!st?.head || !st.chunks.length) return false;
  const doc = view.state.doc;
  const idx = target(changeRanges(st.chunks, doc), doc.lineAt(view.state.selection.main.head).number - 1);
  const chunk = idx == null ? undefined : st.chunks[idx];
  if (!chunk) return false;
  const pos = doc.lineAt(Math.min(chunk.fromB, doc.length)).from;
  view.dispatch({ selection: { anchor: pos }, effects: EditorView.scrollIntoView(pos, { y: "center" }), userEvent: "select" });
  const head = st.head;
  // Below the hunk's last line, like PHPStorm, once the scroll has been applied.
  const last = chunk.fromB === chunk.toB ? pos : doc.lineAt(Math.max(chunk.fromB, Math.min(chunk.endB, doc.length))).from;
  // Wait until the line stops moving (the scroll may be animated).
  let prev: number | undefined;
  let frames = 0;
  const place = () => {
    const coords = view.coordsAtPos(Math.min(last, view.state.doc.length));
    if (coords && (coords.bottom !== prev || frames < 2) && ++frames < 40) {
      prev = coords.bottom;
      requestAnimationFrame(place);
      return;
    }
    const gutter = view.dom.querySelector(".cm-git-change-gutter")?.getBoundingClientRect();
    if (coords) showChangePopup(view, path, chunk, head, { x: (gutter?.right ?? coords.left) + 6, y: coords.bottom + 2 });
  };
  requestAnimationFrame(place);
  return true;
}

function controller(path: string, blameCompartment: Compartment) {
  return ViewPlugin.define((view) => {
    let disposed = false;
    let dispose = () => {};
    const fetchHead = async () => {
      const st = status();
      const entry = st?.entries.find((e) => e.path === path);
      if (entry && (entry.worktree === "untracked" || entry.index === "added")) {
        view.dispatch({ effects: setHeadEffect.of(null) });
        return;
      }
      try {
        const head = await gitApi.show("HEAD", path);
        if (!disposed) view.dispatch({ effects: setHeadEffect.of(head) });
      } catch {
        if (!disposed) view.dispatch({ effects: setHeadEffect.of(null) });
      }
    };
    const fetchBlame = async () => {
      try {
        const blame = await gitApi.blame(path, view.state.doc.toString());
        if (!disposed) view.dispatch({ effects: setBlameEffect.of(blame) });
      } catch {
        if (!disposed) view.dispatch({ effects: setBlameEffect.of(null) });
      }
    };
    createRoot((d) => {
      dispose = d;
      createEffect(
        on(revision, () => {
          // Never dispatch while the view is still being constructed/updated.
          setTimeout(() => {
            if (disposed) return;
            void fetchHead();
            void fetchBlame();
          });
        }),
      );
      createEffect(
        on(
          () => annotated().has(path),
          (on) => view.dispatch({ effects: blameCompartment.reconfigure(on ? blameGutter(path) : []) }),
          { defer: true },
        ),
      );
      createEffect(
        on(fileReload, (r) => {
          if (!r || r.path !== path) return;
          const { text, eol } = r;
          setTabEol(path, eol);
          if (text === view.state.doc.toString()) return;
          replaceDocument(view, text);
          void saveFile(path);
        }, { defer: true }),
      );
    });
    return {
      destroy() {
        disposed = true;
        dispose();
      },
    };
  });
}

/** Right-click on the gutter: "Annotate with Git Blame" like PHPStorm. */
function gutterContextMenu(path: string): Extension {
  const onMenu = (event: MouseEvent) => {
    const target = event.target as HTMLElement;
    if (!target.closest(".cm-gutters")) return;
    event.preventDefault();
    document.querySelector(".git-change-popup")?.remove();
    const el = document.createElement("div");
    el.className = "git-change-popup git-gutter-menu";
    el.style.left = `${event.clientX}px`;
    el.style.top = `${event.clientY}px`;
    const on = annotated().has(path);
    const item = document.createElement("button");
    item.textContent = on ? "Close Annotations" : "Annotate with Git Blame";
    item.onclick = () => {
      el.remove();
      toggleAnnotate(path, !on);
    };
    el.append(item);
    document.body.append(el);
    const close = (e: MouseEvent) => {
      if (!el.contains(e.target as Node)) {
        el.remove();
        document.removeEventListener("mousedown", close, true);
      }
    };
    setTimeout(() => document.addEventListener("mousedown", close, true));
  };
  // Gutters live outside the content DOM, so listen on the editor root.
  return ViewPlugin.define((view) => {
    view.dom.addEventListener("contextmenu", onMenu);
    return { destroy: () => view.dom.removeEventListener("contextmenu", onMenu) };
  });
}

const gitEditorTheme = EditorView.baseTheme({
  ".cm-git-change-gutter": { width: "5px", paddingLeft: "1px" },
  ".cm-git-change-gutter .cm-gutterElement": { padding: "0", cursor: "pointer" },
  ".cm-git-change": { height: "100%", width: "4px", borderRadius: "1px" },
  ".cm-git-added": { background: "var(--vcs-added)" },
  ".cm-git-modified": { background: "var(--vcs-modified)" },
  ".cm-git-deleted": { position: "relative", height: "100%" },
  ".cm-git-deleted::before": {
    content: '""',
    position: "absolute",
    top: "-4px",
    left: "0",
    borderTop: "4px solid transparent",
    borderBottom: "4px solid transparent",
    borderLeft: "5px solid var(--vcs-deleted)",
  },
  ".cm-git-blame-gutter": { background: "var(--bg-panel)", borderRight: "1px solid var(--border)", marginRight: "2px" },
  ".cm-git-blame-gutter .cm-gutterElement": { padding: "0" },
  ".cm-git-blame": {
    display: "flex",
    gap: "8px",
    height: "100%",
    padding: "0 10px 0 8px",
    borderLeft: "3px solid transparent",
    fontFamily: "var(--font-ui)",
    fontSize: "var(--fs-small)",
    color: "var(--fg-muted)",
    whiteSpace: "nowrap",
    cursor: "default",
  },
  ".cm-git-blame:hover": { background: "var(--bg-hover)", color: "var(--fg)" },
  ".cm-git-blame-spacer": { visibility: "hidden" },
  ".cm-git-blame-date": { fontVariantNumeric: "tabular-nums", minWidth: "54px" },
  ".cm-git-blame-author": { overflow: "hidden", textOverflow: "ellipsis", maxWidth: "110px" },
  ".cm-git-age-0": { borderLeftColor: "var(--blame-age-0)", background: "var(--blame-bg-0)" },
  ".cm-git-age-1": { borderLeftColor: "var(--blame-age-1)", background: "var(--blame-bg-1)" },
  ".cm-git-age-2": { borderLeftColor: "var(--blame-age-2)" },
  ".cm-git-age-3": { borderLeftColor: "var(--blame-age-3)" },
  ".cm-git-age-4": { borderLeftColor: "var(--blame-age-4)" },
  ".cm-git-blame-uncommitted": { borderLeftColor: "transparent", background: "transparent" },
  ".cm-git-inline-blame": {
    marginLeft: "3em",
    color: "var(--fg-faint)",
    fontFamily: "var(--font-ui)",
    fontSize: "var(--fs-small)",
    fontStyle: "italic",
    whiteSpace: "pre",
    pointerEvents: "none",
  },
});

/** All git editor extensions for one file. */
export function gitEditorExtensions(path: string): Extension {
  const blameCompartment = new Compartment();
  return [
    changeField,
    blameField,
    blameCompartment.of(annotated().has(path) ? blameGutter(path) : []),
    changeGutter(path),
    inlineBlame,
    gutterContextMenu(path),
    gitEditorTheme,
    controller(path, blameCompartment),
  ];
}
