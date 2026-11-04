/**
 * Three-way merge window ("Merge Revisions for …"), laid out like PHPStorm's: the yours
 * version on the left, the editable Result in the middle (it starts as the base), theirs on
 * the right. Chunks come from the Rust merge (`forge_vcs::merge`); `mergeState.ts` tracks
 * them in the result. Shortcuts (F7 / Shift+F7, the toolbar actions) come through the keymap
 * in the "diff" focus context.
 */
import { createMemo, createSignal, For, onCleanup, onMount, Show } from "solid-js";
import {
  ArrowDown,
  ArrowLeftRight,
  ArrowLeftToLine,
  ArrowRightToLine,
  ArrowUp,
  ChevronsLeft,
  ChevronsRight,
  Columns3,
  WandSparkles,
  X,
} from "lucide-solid";
import { EditorState, StateEffect, StateField, type Extension, type TransactionSpec } from "@codemirror/state";
import { Decoration, EditorView, drawSelection, highlightSpecialChars, keymap, lineNumbers, type DecorationSet } from "@codemirror/view";
import { defaultKeymap, history, historyKeymap } from "@codemirror/commands";
import { forgeEditorTheme, forgeHighlight } from "@/editor/forgeTheme";
import { languageFor } from "@/editor/languages";
import { ensureFileType, isFileTypeResolved } from "@/editor/fileTypes";
import { FileIcon } from "@/shell/FileIcon";
import { askConfirm } from "@/features/contextMenu/dialogs";
import { showContextMenu, item } from "@/features/contextMenu/state";
import type { MergeFile } from "./api";
import { setDiffRequest } from "./overlays";
import { acceptSide, applyMergeResult, setMergeActions } from "./conflicts";
import {
  applyNonConflicting,
  applySide,
  chunkClass,
  countsLabel,
  ignoreSide,
  initialChunks,
  isResolved,
  lineMarks,
  lineOffsets,
  mapLine,
  mergeCounts,
  mergeResultState,
  nextChunk,
  resolveSimpleConflicts,
  resultLines,
  type ResultChunk,
  type Side,
} from "./mergeState";
import { t as tr } from "@/i18n";

const paneTheme = EditorView.theme({
  "&": { height: "100%" },
  ".cm-scroller": { overflow: "auto" },
});

/** Decorations of a read-only side pane, replaced as the result changes. */
const setSideMarks = StateEffect.define<DecorationSet>();
const sideMarks = StateField.define<DecorationSet>({
  create: () => Decoration.none,
  update: (value, tr) => tr.effects.find((e) => e.is(setSideMarks))?.value ?? value.map(tr.changes),
  provide: (f) => EditorView.decorations.from(f),
});

type Pane = "left" | "center" | "right";
interface Shape {
  index: number;
  cls: string;
  sideTop: number;
  sideBottom: number;
  resultTop: number;
  resultBottom: number;
  /** The other side of a conflict is applied: the arrow appends. */
  append: boolean;
}

const DIVIDER = 40;

export function MergeWindow(props: { file: MergeFile; onClose: () => void }) {
  const file = props.file;
  const chunks = file.model!.chunks;
  const base = file.base ?? "";
  const texts = { left: file.yours ?? "", right: file.theirs ?? "" };
  const offsets = { left: lineOffsets(texts.left), right: lineOffsets(texts.right) };
  const result = mergeResultState(chunks, base);

  let root!: HTMLDivElement;
  const hosts: Partial<Record<Pane, HTMLDivElement>> = {};
  const views: Partial<Record<Pane, EditorView>> = {};
  const dividers: Partial<Record<Side, HTMLDivElement>> = {};
  const [list, setList] = createSignal<ResultChunk[]>(initialChunks(chunks, base));
  const [tick, setTick] = createSignal(0);
  const [modified, setModified] = createSignal(false);
  const counts = createMemo(() => mergeCounts(list(), chunks));
  const bump = () => setTick((t) => t + 1);

  const common = (editable: boolean): Extension[] => [
    lineNumbers(),
    highlightSpecialChars(),
    drawSelection(),
    forgeEditorTheme,
    forgeHighlight,
    paneTheme,
    ...languageFor(file.path),
    EditorView.editable.of(editable),
    EditorState.readOnly.of(!editable),
    EditorView.updateListener.of((u) => {
      if (u.geometryChanged || u.viewportChanged || u.heightChanged) bump();
    }),
  ];

  /** Colour the changes of a side that are not handled yet. */
  const markSide = (side: Side, chunkList: ResultChunk[]) => {
    const view = views[side];
    if (!view) return;
    const at = offsets[side];
    const ranges = chunks
      .map((ch, i): [number, number, string] | null => {
        const changed = side === "left" ? ch.kind !== "right" : ch.kind !== "left";
        const c = chunkList[i];
        if (!changed || !c || c[side] || isResolved(c)) return null;
        const r = ch[side];
        return [at[r.start], at[r.end], chunkClass(ch.kind, ch.baseText, side === "left" ? ch.leftText : ch.rightText)];
      })
      .filter((x): x is [number, number, string] => x !== null);
    view.dispatch({ effects: setSideMarks.of(lineMarks(view.state, ranges)) });
  };

  const syncState = () => {
    const cv = views.center;
    if (!cv) return;
    const next = cv.state.field(result.field);
    setList(next);
    markSide("left", next);
    markSide("right", next);
    bump();
  };

  const dispatch = (spec: TransactionSpec | null) => {
    const cv = views.center;
    if (!spec || !cv) return;
    cv.dispatch(spec);
    cv.focus();
  };

  // ---- navigation and actions (toolbar, dividers, keymap) ------------------------------

  const goTo = (index: number) => {
    const cv = views.center;
    const c = cv?.state.field(result.field)[index];
    if (!cv || !c) return;
    cv.dispatch({ selection: { anchor: c.from }, effects: EditorView.scrollIntoView(c.from, { y: "center" }) });
    cv.focus();
  };
  const step = (forward: boolean) => {
    const cv = views.center;
    if (!cv) return;
    const index = nextChunk(cv.state.field(result.field), cv.state.selection.main.head, forward);
    if (index >= 0) goTo(index);
  };
  const apply = (index: number, side: Side) => dispatch(views.center ? applySide(views.center.state, result.field, chunks, index, side) : null);
  const ignore = (index: number, side: Side) => dispatch(views.center ? ignoreSide(views.center.state, result.field, chunks, index, side) : null);
  const applyAll = (which: Side | "all") => dispatch(views.center ? applyNonConflicting(views.center.state, result.field, chunks, which) : null);
  const magic = () => dispatch(views.center ? resolveSimpleConflicts(views.center.state, result.field, chunks) : null);

  const finish = async () => {
    const cv = views.center;
    if (!cv) return;
    const c = counts();
    if (c.changes + c.conflicts > 0) {
      const ok = await askConfirm({
        title: tr("git.mergeWindow.unresolvedChanges"),
        message: tr("git.mergeWindow.thereAreUnresolvedChangesApply"),
        confirm: tr("common.apply"),
      });
      if (!ok) return;
    }
    if (await applyMergeResult(file, cv.state.doc.toString())) props.onClose();
  };
  const cancel = async () => {
    if (modified()) {
      const ok = await askConfirm({ title: tr("git.mergeWindow.cancelMerge"), message: tr("git.mergeWindow.discardTheChangesMadeIn"), confirm: tr("git.mergeWindow.discard") });
      if (!ok) return;
    }
    props.onClose();
  };
  const acceptWhole = async (side: "yours" | "theirs") => {
    if (await acceptSide([file.path], side)) props.onClose();
  };

  const compare = (e: MouseEvent) => {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const resultText = () => views.center?.state.doc.toString() ?? base;
    const show = (left: { title: string; content: string }, right: { title: string; content: string }) =>
      setDiffRequest({ path: file.path, left, right, editable: false });
    showContextMenu(
      [
        item(tr("git.mergeWindow.compareLeftAndBase"), () => show({ title: file.yoursTitle, content: texts.left }, { title: tr("git.mergeWindow.base"), content: base })),
        item(tr("git.mergeWindow.compareBaseAndRight"), () => show({ title: tr("git.mergeWindow.base"), content: base }, { title: file.theirsTitle, content: texts.right })),
        item(tr("git.mergeWindow.compareLeftAndRight"), () => show({ title: file.yoursTitle, content: texts.left }, { title: file.theirsTitle, content: texts.right })),
        item(tr("git.mergeWindow.compareBaseAndResult"), () => show({ title: tr("git.mergeWindow.base"), content: base }, { title: tr("git.mergeWindow.result"), content: resultText() })),
      ],
      r.left,
      r.bottom + 2,
    );
  };

  // ---- scrolling the three panes together ------------------------------------------------

  /** `[fromStart, fromEnd, toStart, toEnd]` line anchors from `from` to `to`. */
  const anchors = (from: Pane, to: Pane): [number, number, number, number][] => {
    const cv = views.center;
    if (!cv) return [];
    const list = cv.state.field(result.field);
    const range = (pane: Pane, i: number): [number, number] => {
      if (pane === "center") return resultLines(cv.state, list[i].from, list[i].to);
      const r = chunks[i][pane];
      return [r.start, r.end];
    };
    return chunks.map((_, i) => [...range(from, i), ...range(to, i)] as [number, number, number, number]);
  };

  let syncing = false;
  const sync = (from: Pane) => {
    if (syncing) return;
    const src = views[from];
    if (!src) return;
    syncing = true;
    const top = src.scrollDOM.scrollTop;
    const block = src.lineBlockAtHeight(top);
    const line = src.state.doc.lineAt(block.from).number - 1;
    const offset = top - block.top;
    for (const to of ["left", "center", "right"] as Pane[]) {
      const target = views[to];
      if (to === from || !target) continue;
      const mapped =
        from === "center" || to === "center"
          ? mapLine(line, anchors(from, to))
          : mapLine(mapLine(line, anchors(from, "center")), anchors("center", to));
      const n = Math.min(Math.max(mapped + 1, 1), target.state.doc.lines);
      target.scrollDOM.scrollTop = target.lineBlockAt(target.state.doc.line(n).from).top + offset;
    }
    requestAnimationFrame(() => {
      syncing = false;
      bump();
    });
  };

  // ---- connectors and chunk buttons between the panes ---------------------------------

  /** Vertical extent of `from..to` in `view`, relative to `el`'s top. */
  const extent = (view: EditorView, el: HTMLElement, from: number, to: number): [number, number] => {
    const doc = view.state.doc;
    const dy = view.documentTop - el.getBoundingClientRect().top;
    if (from === to) {
      const line = doc.lineAt(Math.min(from, doc.length));
      const block = view.lineBlockAt(line.from);
      const y = from > line.from ? block.bottom : block.top;
      return [y + dy, y + dy];
    }
    const first = view.lineBlockAt(from);
    const last = view.lineBlockAt(Math.max(from, to - 1));
    return [first.top + dy, last.bottom + dy];
  };

  const shapes = (side: Side): Shape[] => {
    tick();
    const el = dividers[side];
    const sv = views[side];
    const cv = views.center;
    if (!el || !sv || !cv) return [];
    const at = offsets[side];
    const height = el.clientHeight;
    const out: Shape[] = [];
    list().forEach((c, index) => {
      const ch = chunks[index];
      const changed = side === "left" ? ch.kind !== "right" : ch.kind !== "left";
      if (!changed || isResolved(c) || c[side]) return;
      const r = ch[side];
      const [sideTop, sideBottom] = extent(sv, el, at[r.start], at[r.end]);
      const [resultTop, resultBottom] = extent(cv, el, c.from, c.to);
      if (Math.max(sideBottom, resultBottom) < -20 || Math.min(sideTop, resultTop) > height + 20) return;
      const other: Side = side === "left" ? "right" : "left";
      out.push({
        index,
        cls: chunkClass(ch.kind, ch.baseText, side === "left" ? ch.leftText : ch.rightText),
        sideTop,
        sideBottom,
        resultTop,
        resultBottom,
        append: ch.kind === "conflict" && c.applied && c[other],
      });
    });
    return out;
  };

  /** A band from the side pane to the result, curved like PHPStorm's. */
  const band = (s: Shape, side: Side) => {
    const w = DIVIDER;
    const [y1, y2, y3, y4] = side === "left" ? [s.sideTop, s.resultTop, s.resultBottom, s.sideBottom] : [s.resultTop, s.sideTop, s.sideBottom, s.resultBottom];
    const m = w / 2;
    return `M0,${y1} C${m},${y1} ${m},${y2} ${w},${y2} L${w},${y3} C${m},${y3} ${m},${y4} 0,${y4} Z`;
  };

  const Divider = (p: { side: Side }) => {
    const list = createMemo(() => shapes(p.side));
    return (
    <div class="merge-divider" ref={(el) => (dividers[p.side] = el)}>
      <svg class="merge-bands" width={DIVIDER} height="100%">
        <For each={list()}>{(s) => <path class={`merge-band merge-band-${s.cls}`} d={band(s, p.side)} />}</For>
      </svg>
      <For each={list()}>
        {(s) => (
          <div class="merge-chunk-actions" classList={{ right: p.side === "right" }} style={{ top: `${Math.max(0, s.sideTop)}px` }}>
            <Show when={p.side === "right"}>
              <button class="merge-chunk-btn" title={tr("git.mergeWindow.ignore")} onClick={() => ignore(s.index, "right")}>
                <X size={12} />
              </button>
            </Show>
            <button
              class="merge-chunk-btn apply"
              title={s.append ? (p.side === "left" ? tr("git.mergeWindow.appendLeft") : tr("git.mergeWindow.appendRight")) : p.side === "left" ? tr("git.mergeWindow.acceptLeft") : tr("git.mergeWindow.acceptRight")}
              onClick={() => apply(s.index, p.side)}
            >
              {p.side === "left" ? <ChevronsRight size={14} /> : <ChevronsLeft size={14} />}
            </button>
            <Show when={p.side === "left"}>
              <button class="merge-chunk-btn" title={tr("git.mergeWindow.ignore")} onClick={() => ignore(s.index, "left")}>
                <X size={12} />
              </button>
            </Show>
          </div>
        )}
      </For>
    </div>
    );
  };

  // ---- lifecycle -----------------------------------------------------------------------------

  let disposed = false;
  const build = () => {
    if (disposed) return;
    views.left = new EditorView({
      parent: hosts.left!,
      state: EditorState.create({ doc: texts.left, extensions: [...common(false), sideMarks] }),
    });
    views.right = new EditorView({
      parent: hosts.right!,
      state: EditorState.create({ doc: texts.right, extensions: [...common(false), sideMarks] }),
    });
    views.center = new EditorView({
      parent: hosts.center!,
      state: EditorState.create({
        doc: base,
        extensions: [
          ...common(true),
          result.extension,
          history(),
          keymap.of([...defaultKeymap, ...historyKeymap]),
          EditorView.updateListener.of((u) => {
            if (u.docChanged) setModified(u.state.doc.toString() !== base);
            if (u.transactions.length) syncState();
          }),
        ],
      }),
    });
    for (const pane of ["left", "center", "right"] as Pane[]) views[pane]!.scrollDOM.addEventListener("scroll", () => sync(pane));
    syncState();
    const first = nextChunk(views.center.state.field(result.field), -1, true);
    if (first >= 0) goTo(first);
    else views.center.focus();
  };

  onMount(() => {
    setMergeActions({ step, applyAll, magic });
    if (isFileTypeResolved(file.path)) build();
    else void ensureFileType(file.path).then(build);
    const ro = new ResizeObserver(bump);
    ro.observe(root);
    onCleanup(() => ro.disconnect());
  });
  onCleanup(() => {
    disposed = true;
    setMergeActions(null);
    for (const v of Object.values(views)) v?.destroy();
  });

  const name = file.path.split("/").pop() ?? file.path;
  return (
    <div class="modal-backdrop">
      <div
        class="diff-viewer merge-window"
        role="dialog"
        aria-label={tr("git.mergeWindow.mergeRevisionsForPath", { path: file.path })}
        data-focus-context="diff"
        tabIndex={-1}
        ref={root}
        onKeyDown={(e) => {
          if (e.key === "Escape" && !e.defaultPrevented) {
            e.preventDefault();
            void cancel();
          }
        }}
      >
        <header class="dialog-title">
          <FileIcon name={name} size={14} />
          <span>{tr("git.mergeWindow.mergeRevisionsForPath", { path: file.path })}</span>
          <span class="spacer" />
          <button class="icon-btn" title={tr("git.mergeWindow.cancel", { shortcut: "Escape" })} onClick={() => void cancel()}>
            <X size={15} />
          </button>
        </header>
        <div class="panel-toolbar diff-toolbar">
          <button class="icon-btn" title={tr("git.mergeWindow.previousDifference", { shortcut: "Shift+F7" })} onClick={() => step(false)}>
            <ArrowUp size={15} />
          </button>
          <button class="icon-btn" title={tr("git.mergeWindow.nextDifference", { shortcut: "F7" })} onClick={() => step(true)}>
            <ArrowDown size={15} />
          </button>
          <div class="toolbar-sep" />
          <button class="icon-btn" title={tr("git.mergeWindow.applyNonConflictingChangesFrom")} disabled={!counts().left} onClick={() => applyAll("left")}>
            <ArrowRightToLine size={15} />
          </button>
          <button
            class="icon-btn"
            title={tr("git.mergeWindow.applyAllNonConflictingChanges")}
            disabled={!counts().left && !counts().right}
            onClick={() => applyAll("all")}
          >
            <ArrowLeftRight size={15} />
          </button>
          <button class="icon-btn" title={tr("git.mergeWindow.applyNonConflictingChangesFrom2")} disabled={!counts().right} onClick={() => applyAll("right")}>
            <ArrowLeftToLine size={15} />
          </button>
          <button class="icon-btn" title={tr("git.mergeWindow.resolveSimpleConflicts")} disabled={!counts().simple} onClick={magic}>
            <WandSparkles size={15} />
          </button>
          <div class="toolbar-sep" />
          <button class="icon-btn" title={tr("git.mergeWindow.compareContents")} onClick={compare}>
            <Columns3 size={15} />
          </button>
          <span class="spacer" />
          <span class="diff-count merge-count" classList={{ done: counts().changes + counts().conflicts === 0 }}>
            {countsLabel(counts())}
            <Show when={counts().changes + counts().conflicts === 0}>
              {" "}
              <button class="link-btn" onClick={() => void finish()}>
                {tr("git.mergeWindow.saveChangesAndFinishMerging")}
              </button>
            </Show>
          </span>
        </div>
        <div class="merge-headers">
          <div class="diff-header" title={file.yoursTitle}>
            {file.yoursTitle}
          </div>
          <div class="diff-header merge-header-result">{tr("git.mergeWindow.result")}</div>
          <div class="diff-header" title={file.theirsTitle}>
            {file.theirsTitle}
          </div>
        </div>
        <div class="merge-body">
          <div class="merge-pane merge-pane-side" ref={(el) => (hosts.left = el)} />
          <Divider side="left" />
          <div class="merge-pane merge-pane-result" ref={(el) => (hosts.center = el)} />
          <Divider side="right" />
          <div class="merge-pane merge-pane-side" ref={(el) => (hosts.right = el)} />
        </div>
        <footer class="merge-footer">
          <button class="btn-secondary" onClick={() => void acceptWhole("yours")}>
            {tr("git.mergeWindow.acceptLeft")}
          </button>
          <button class="btn-secondary" onClick={() => void acceptWhole("theirs")}>
            {tr("git.mergeWindow.acceptRight")}
          </button>
          <span class="spacer" />
          <button class="btn-secondary" onClick={() => void cancel()}>
            Cancel
          </button>
          <button class="btn-primary" onClick={() => void finish()}>
            Apply
          </button>
        </footer>
      </div>
    </div>
  );
}
