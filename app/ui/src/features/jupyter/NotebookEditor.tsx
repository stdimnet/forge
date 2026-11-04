/**
 * The notebook editor (PyCharm's Jupyter editor): a toolbar with Run All, Interrupt,
 * Restart, the cell type and the kernel's state, the cells (CodeMirror editors and
 * rendered Markdown) with their outputs, and the Variables view of the kernel.
 * Command mode (a cell selected) and edit mode (a cell's editor focused) follow Jupyter;
 * their shortcuts come from the keymap.
 */
import { createEffect, createResource, For, on, onCleanup, onMount, Show } from "solid-js";
import {
  ChevronDown,
  ChevronRight,
  Circle,
  Eraser,
  FastForward,
  LoaderCircle,
  Play,
  Plus,
  Power,
  RotateCw,
  Settings2,
  Sheet,
  Square,
  Trash2,
  Variable,
} from "lucide-solid";
import type { EditorTab } from "@/state/editor";
import { editor } from "@/state/editor";
import type { CellType, JupyterVariable } from "@/ipc/jupyter";
import { markdownApi } from "@/ipc/markdown";
import { Markdown } from "@/features/ai-chat/Markdown";
import { openSettings } from "@/features/settings/dialogState";
import { CellEditor } from "./CellEditor";
import { Outputs } from "./Outputs";
import { countLabel, kernelLabel, type CellState } from "./model";
import { notebookFor, releaseNotebook, type NotebookController } from "./store";
import { openFrameTab } from "./FrameTab";
import { t as tr } from "@/i18n";

function KernelStatusView(props: { ctl: NotebookController }) {
  const k = () => props.ctl.state.kernel;
  return (
    <span class="nb-kernel" classList={{ [k().status]: true }} title={k().error ?? (k().label ? `${k().label}: ${kernelLabel(k().status)}` : tr("langs.jupyter.startsWithTheFirstCell"))}>
      <Show when={k().status === "starting" || k().status === "restarting" || k().status === "busy"} fallback={<Circle size={9} class="nb-kernel-dot" />}>
        <LoaderCircle size={12} class="spin" />
      </Show>
      <span class="nb-kernel-name">{k().label || tr("langs.jupyter.pythonKernel")}</span>
      <span class="muted">{kernelLabel(k().status)}</span>
    </span>
  );
}

function Toolbar(props: { ctl: NotebookController }) {
  const s = props.ctl.state;
  const cellType = () => s.cells[s.selected]?.cellType ?? "code";
  const running = () => s.cells.some((c) => c.running);
  return (
    <div class="nb-toolbar" role="toolbar" aria-label={tr("langs.jupyter.notebook")}>
      <button class="icon-btn" title={tr("langs.jupyter.runCellAndSelectBelow")} onClick={() => void props.ctl.run(s.selected, "next")}>
        <Play size={14} />
      </button>
      <button class="icon-btn" title={tr("langs.jupyter.runAllCells")} onClick={() => void props.ctl.runAll()}>
        <FastForward size={14} />
      </button>
      <button class="icon-btn" title={tr("langs.jupyter.interruptKernel")} disabled={!s.kernel.id || !running()} onClick={() => void props.ctl.interrupt()}>
        <Square size={13} />
      </button>
      <button class="icon-btn" title={tr("langs.jupyter.restartKernel")} disabled={!s.kernel.id} onClick={() => void props.ctl.restart()}>
        <RotateCw size={14} />
      </button>
      <button class="icon-btn" title={tr("langs.jupyter.clearAllOutputs")} onClick={() => props.ctl.clearOutputs()}>
        <Eraser size={14} />
      </button>
      <span class="toolbar-sep" />
      <button class="icon-btn" title={tr("langs.jupyter.addCodeCellBelowB")} onClick={() => props.ctl.insert(s.selected + 1, "code")}>
        <Plus size={14} />
      </button>
      <button class="icon-btn" title={tr("langs.jupyter.deleteCellDD")} disabled={!s.cells.length} onClick={() => props.ctl.remove()}>
        <Trash2 size={14} />
      </button>
      <select
        class="nb-type-select"
        title={tr("langs.jupyter.cellTypeYMR")}
        value={cellType()}
        onChange={(e) => props.ctl.setType(e.currentTarget.value as CellType)}
      >
        <option value="code">{tr("langs.jupyter.code")}</option>
        <option value="markdown">Markdown</option>
        <option value="raw">{tr("langs.jupyter.raw")}</option>
      </select>
      <span class="spacer" />
      <button
        class="icon-btn"
        classList={{ active: s.showVariables }}
        title={tr("langs.jupyter.showVariables")}
        aria-pressed={s.showVariables}
        onClick={() => props.ctl.toggleVariables()}
      >
        <Variable size={14} />
      </button>
      <KernelStatusView ctl={props.ctl} />
      <button class="icon-btn" title={tr("langs.jupyter.shutDownKernel")} disabled={!s.kernel.id} onClick={() => void props.ctl.shutdown()}>
        <Power size={13} />
      </button>
      <button class="icon-btn" title={tr("langs.jupyter.jupyterSettings")} onClick={() => openSettings("lang.jupyter")}>
        <Settings2 size={14} />
      </button>
    </div>
  );
}

function MarkdownCell(props: { ctl: NotebookController; cell: CellState }) {
  const [blocks] = createResource(
    () => (props.cell.editing ? null : props.cell.source),
    (t) => markdownApi.parse(t, "preview"),
  );
  return (
    <Show
      when={!props.cell.editing && props.cell.source.trim()}
      fallback={
        <CellEditor ctl={props.ctl} cellId={props.cell.id} cellType="markdown" source={props.cell.source} language="markdown" autofocus={props.ctl.state.mode === "edit"} />
      }
    >
      <div class="nb-md" onDblClick={() => props.ctl.enterEdit(props.ctl.cellIndex(props.cell.id))}>
        <Markdown blocks={blocks.latest ?? []} />
      </div>
    </Show>
  );
}

function CellView(props: { ctl: NotebookController; cell: CellState; index: number }) {
  const s = props.ctl.state;
  const selected = () => s.selected === props.index;
  return (
    <div
      class="nb-cell"
      classList={{
        selected: selected(),
        editing: selected() && s.mode === "edit",
        running: props.cell.running,
        failed: props.cell.status === "error",
        [props.cell.cellType]: true,
      }}
      data-cell-index={props.index}
      data-cell-id={props.cell.id}
      data-testid="notebook-cell"
      onMouseDown={(e) => {
        // Clicks outside the cell's editor select it in command mode.
        if (!(e.target as Element).closest(".cm-editor")) props.ctl.select(props.index);
      }}
    >
      <div class="nb-cell-gutter">
        <Show when={props.cell.cellType === "code"}>
          <button
            class="nb-run"
            title={props.cell.running ? tr("langs.jupyter.runningEllipsis") : tr("langs.jupyter.runCell")}
            onMouseDown={(e) => e.stopPropagation()}
            onClick={() => void props.ctl.run(props.index)}
          >
            <Show when={props.cell.running} fallback={<Play size={12} />}>
              <LoaderCircle size={12} class="spin" />
            </Show>
          </button>
          <span class="nb-count">{countLabel(props.cell)}</span>
        </Show>
      </div>
      <div class="nb-cell-main">
        <div class="nb-input">
          <Show when={props.cell.cellType === "markdown"} fallback={
            <Show keyed when={props.cell.cellType}>
              {(type) => <CellEditor ctl={props.ctl} cellId={props.cell.id} cellType={type} source={props.cell.source} language={s.meta.language} />}
            </Show>
          }>
            <MarkdownCell ctl={props.ctl} cell={props.cell} />
          </Show>
        </div>
        <Show when={props.cell.cellType === "code"}>
          <Outputs rendered={props.cell.rendered} />
        </Show>
      </div>
    </div>
  );
}

function VariableRows(props: { ctl: NotebookController; vars: JupyterVariable[]; depth: number }) {
  const s = props.ctl.state;
  return (
    <For each={props.vars}>
      {(v) => (
        <>
          <div class="pycon-var tree-row nb-var" style={{ "padding-left": `${6 + props.depth * 14}px` }} title={`${v.name} = ${v.value}`}>
            <span class="pycon-var-toggle" onClick={() => v.children && void props.ctl.toggleExpanded(v)}>
              <Show when={v.children}>{s.expanded[v.expr] ? <ChevronDown size={12} /> : <ChevronRight size={12} />}</Show>
            </span>
            <span class="pycon-var-name">{v.name}</span>
            <span class="pycon-var-eq"> = </span>
            <span class="pycon-var-type muted" title={v.type}>
              {"{"}
              {v.type.split(".").pop()}
              {"}"}
              {v.size != null ? ` ${v.size}` : ""}
            </span>
            <span class="pycon-var-value">{v.value}</span>
            <Show when={v.frame}>
              <button class="nb-var-view" title={tr("langs.jupyter.viewAsDataframe")} onClick={() => void openFrameTab(props.ctl, v.expr)}>
                <Sheet size={12} /> {tr("langs.jupyter.view")}
              </button>
            </Show>
          </div>
          <Show when={s.expanded[v.expr] && s.children[v.expr]}>
            <VariableRows ctl={props.ctl} vars={s.children[v.expr]} depth={props.depth + 1} />
          </Show>
        </>
      )}
    </For>
  );
}

function VariablesPanel(props: { ctl: NotebookController }) {
  const s = props.ctl.state;
  return (
    <div class="nb-vars" data-testid="jupyter-variables">
      <div class="pycon-vars-head">
        <Variable size={12} /> {tr("langs.jupyter.variables")}
        <span class="spacer" />
        <button class="icon-btn" title={tr("common.refresh")} disabled={!s.kernel.id} onClick={() => void props.ctl.refreshVariables()}>
          <RotateCw size={12} />
        </button>
      </div>
      <div class="pycon-vars-list">
        <Show
          when={s.vars.length}
          fallback={<div class="muted pycon-vars-empty">{s.kernel.id ? tr("langs.jupyter.noVariablesYet") : tr("langs.jupyter.runACellToStart")}</div>}
        >
          <VariableRows ctl={props.ctl} vars={s.vars} depth={0} />
        </Show>
      </div>
    </div>
  );
}

export function NotebookEditor(props: { tab: EditorTab; group?: number }) {
  const path = props.tab.path;
  const ctl = notebookFor(path);
  const s = ctl.state;
  let container!: HTMLDivElement;

  // (Re)load when the file changes on disk (checkout, reload) — not for our own edits.
  createEffect(
    on(
      () => props.tab.content,
      (text) => {
        if (!s.loaded || !ctl.isOwnText(text)) void ctl.load(text);
      },
    ),
  );
  onMount(() => {
    ctl.container = container;
  });
  onCleanup(() => {
    if (ctl.container === container) ctl.container = null;
    // Closed in every group: stop the kernel (setting) and forget the notebook.
    queueMicrotask(() => {
      if (!editor.tabs.some((t) => t.path === path)) releaseNotebook(path);
    });
  });

  return (
    <div class="nb-editor" data-testid="notebook-editor">
      <Toolbar ctl={ctl} />
      <div class="nb-body">
        <div
          ref={container}
          class="nb-cells"
          tabIndex={0}
          data-focus-context="notebook"
          onMouseDown={(e) => {
            if (e.target === container) ctl.select(s.selected);
          }}
        >
          <Show when={s.error}>
            <div class="nb-error">
              <div>{s.error}</div>
              <div class="muted">{tr("langs.jupyter.openTheFileAsText")}</div>
            </div>
          </Show>
          <For each={s.cells}>{(cell, i) => <CellView ctl={ctl} cell={cell} index={i()} />}</For>
          <Show when={s.loaded && !s.error}>
            <div class="nb-add">
              <button class="pill-btn" onClick={() => ctl.insert(s.cells.length, "code")}>
                <Plus size={12} /> {tr("langs.jupyter.code")}
              </button>
              <button class="pill-btn" onClick={() => ctl.insert(s.cells.length, "markdown")}>
                <Plus size={12} /> Markdown
              </button>
            </div>
          </Show>
        </div>
        <Show when={s.showVariables}>
          <VariablesPanel ctl={ctl} />
        </Show>
      </div>
    </div>
  );
}
