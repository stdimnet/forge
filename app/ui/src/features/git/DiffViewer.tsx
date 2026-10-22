/** Side-by-side diff viewer (Show Diff, Ctrl+D) built on @codemirror/merge. */
import { createSignal, onCleanup, onMount, Show } from "solid-js";
import { ArrowDown, ArrowLeft, ArrowRight, ArrowUp, FoldVertical, SquareArrowOutUpRight, X } from "lucide-solid";
import { EditorState, type Extension } from "@codemirror/state";
import { EditorView, lineNumbers, highlightSpecialChars, drawSelection, keymap } from "@codemirror/view";
import { defaultKeymap, history, historyKeymap } from "@codemirror/commands";
import { MergeView, goToNextChunk, goToPreviousChunk } from "@codemirror/merge";
import { forgeEditorTheme, forgeHighlight } from "@/editor/forgeTheme";
import { languageFor } from "@/editor/languages";
import { ensureFileType, isFileTypeResolved } from "@/editor/fileTypes";
import { openFileReported } from "@/features/navigation/openReported";
import { workspaceApi } from "@/ipc/workspace";
import { FileIcon } from "@/shell/FileIcon";
import { reloadOpenFiles, repositoryChanged } from "./store";
import { showDiff } from "./actions";
import type { DiffRequest } from "./overlays";
import { lineDiffConfig } from "./lineDiff";

const diffTheme = EditorView.theme({
  "&": { height: "auto" },
  ".cm-scroller": { overflow: "visible" },
});

function revertButton(): HTMLElement {
  const b = document.createElement("button");
  b.className = "diff-revert";
  b.textContent = "»";
  b.title = "Revert: replace with the left side";
  return b;
}

export function DiffViewer(props: { request: DiffRequest; onClose: () => void }) {
  let host!: HTMLDivElement;
  let mv: MergeView | undefined;
  const [count, setCount] = createSignal(0);
  const [collapse, setCollapse] = createSignal(true);
  const req = props.request;

  const base = (editable: boolean): Extension[] => [
    lineNumbers(),
    highlightSpecialChars(),
    drawSelection(),
    forgeEditorTheme,
    forgeHighlight,
    diffTheme,
    ...languageFor(req.path),
    EditorView.editable.of(editable),
    EditorState.readOnly.of(!editable),
  ];

  const build = () => {
    const right = mv ? mv.b.state.doc.toString() : req.right.content;
    mv?.destroy();
    mv = new MergeView({
      parent: host,
      a: { doc: req.left.content, extensions: base(false) },
      b: {
        doc: right,
        extensions: [
          ...base(req.editable),
          history(),
          keymap.of([...defaultKeymap, ...historyKeymap]),
          EditorView.updateListener.of(() => queueMicrotask(() => setCount(mv?.chunks.length ?? 0))),
        ],
      },
      revertControls: req.editable ? "a-to-b" : undefined,
      renderRevertControl: revertButton,
      highlightChanges: true,
      gutter: true,
      diffConfig: lineDiffConfig,
      collapseUnchanged: collapse() ? { margin: 3, minSize: 6 } : undefined,
    });
    setCount(mv.chunks.length);
  };

  /** Write edits made on the right side (reverted chunks) back to the file. */
  const saveEdits = async () => {
    const edited = mv && req.editable ? mv.b.state.doc.toString() : null;
    if (edited !== null && edited !== req.right.content) {
      await workspaceApi.writeFile(req.path, edited);
      await reloadOpenFiles([req.path]);
      await repositoryChanged();
    }
  };

  const close = async () => {
    await saveEdits();
    props.onClose();
  };

  const nav = (dir: 1 | -1) => {
    if (!mv) return;
    const view = mv.b;
    (dir > 0 ? goToNextChunk : goToPreviousChunk)({ state: view.state, dispatch: (tr) => view.dispatch(tr) });
    view.focus();
  };

  const files = req.files ?? [];
  const fileIdx = files.indexOf(req.path);
  const stepFile = (d: number) => {
    const next = files[fileIdx + d];
    if (next) void saveEdits().then(() => showDiff(next, files));
  };

  const onKey = (e: KeyboardEvent) => {
    if (e.key === "Escape") {
      e.preventDefault();
      void close();
    } else if (e.key === "F7" || (e.altKey && e.key === "ArrowDown")) {
      e.preventDefault();
      nav(e.shiftKey ? -1 : 1);
    } else if (e.altKey && e.key === "ArrowUp") {
      e.preventDefault();
      nav(-1);
    }
  };

  let disposed = false;
  onMount(() => {
    // The syntax mode depends on the file type, which may still be resolving (desktop app).
    if (isFileTypeResolved(req.path)) build();
    else void ensureFileType(req.path).then(() => disposed || build());
    window.addEventListener("keydown", onKey, true);
  });
  onCleanup(() => {
    disposed = true;
    window.removeEventListener("keydown", onKey, true);
    mv?.destroy();
  });

  const name = req.path.split("/").pop() ?? req.path;
  return (
    <div class="modal-backdrop" onMouseDown={(e) => e.target === e.currentTarget && void close()}>
      <div class="diff-viewer" role="dialog" aria-label={`Diff for ${req.path}`}>
        <header class="dialog-title">
          <FileIcon name={name} size={14} />
          <span>{req.path}</span>
          <span class="spacer" />
          <button class="icon-btn" title="Close (Escape)" onClick={() => void close()}>
            <X size={15} />
          </button>
        </header>
        <div class="panel-toolbar diff-toolbar">
          <button class="icon-btn" title="Previous Difference (Shift+F7)" onClick={() => nav(-1)}>
            <ArrowUp size={15} />
          </button>
          <button class="icon-btn" title="Next Difference (F7)" onClick={() => nav(1)}>
            <ArrowDown size={15} />
          </button>
          <Show when={files.length > 1}>
            <button class="icon-btn" title="Compare Previous File (Alt+Left)" disabled={fileIdx <= 0} onClick={() => stepFile(-1)}>
              <ArrowLeft size={15} />
            </button>
            <button class="icon-btn" title="Compare Next File (Alt+Right)" disabled={fileIdx >= files.length - 1} onClick={() => stepFile(1)}>
              <ArrowRight size={15} />
            </button>
          </Show>
          <button
            class="icon-btn"
            title="Jump to Source (F4)"
            onClick={() => {
              void close();
              void openFileReported(req.path);
            }}
          >
            <SquareArrowOutUpRight size={15} />
          </button>
          <div class="toolbar-sep" />
          <button class="pill-btn diff-select" title="Viewer">
            Side-by-side viewer
          </button>
          <button class="pill-btn diff-select" title="Whitespace">
            Do not ignore
          </button>
          <button class="pill-btn diff-select" title="Highlighting">
            Highlight words
          </button>
          <button
            class="icon-btn"
            classList={{ active: collapse() }}
            title="Collapse Unchanged Fragments"
            onClick={() => {
              setCollapse(!collapse());
              build();
            }}
          >
            <FoldVertical size={15} />
          </button>
          <span class="spacer" />
          <span class="diff-count">{count() === 0 ? "No differences" : `${count()} difference${count() === 1 ? "" : "s"}`}</span>
        </div>
        <div class="diff-headers">
          <div class="diff-header">{req.left.title}</div>
          <div class="diff-header">{req.right.title}</div>
        </div>
        <div class="diff-body" ref={host} />
      </div>
    </div>
  );
}
