/**
 * One notebook in the editor: its cells, the selection and mode (command / edit), its
 * kernel and the Variables view. The `.ipynb` text is kept in the editor tab (serialized
 * in Rust after every change), so Save, the dirty marker and Local History work as for
 * any file. Kernels run in Rust (`jupyter_*`); their events arrive per cell id.
 */
import { batch } from "solid-js";
import { createStore, produce, type SetStoreFunction } from "solid-js/store";
import type { EditorView } from "@codemirror/view";
import {
  jupyterApi,
  type CellType,
  type JupyterVariable,
  type KernelEvent,
  type Notebook,
  type Output,
  type Rendered,
} from "@/ipc/jupyter";
import { pythonApi } from "@/ipc/python";
import { askConfirmWithCheckbox } from "@/features/contextMenu/dialogs";
import { balloonsFor } from "@/keymap/dispatch";
import { editor, updateContent } from "@/state/editor";
import { getSetting, saveSettings } from "@/state/settings";
import { openSettings } from "@/features/settings/dialogState";
import { commandName, runCommandLine } from "@/features/python/state";
import { t } from "@/i18n";
import { moveTarget, newCell, selectionAfterRemove, toCellState, toNotebook, type CellState } from "./model";

export const showJupyterBalloon = balloonsFor("Jupyter");

export type KernelStatus = "none" | "starting" | "idle" | "busy" | "restarting" | "dead" | "failed";

export interface KernelState {
  id: string | null;
  label: string;
  status: KernelStatus;
  error?: string;
  server: boolean;
}

export interface NotebookState {
  loaded: boolean;
  error: string | null;
  meta: Omit<Notebook, "cells">;
  cells: CellState[];
  selected: number;
  mode: "command" | "edit";
  kernel: KernelState;
  showVariables: boolean;
  vars: JupyterVariable[];
  children: Record<string, JupyterVariable[]>;
  expanded: Record<string, boolean>;
}

const EMPTY_META: Omit<Notebook, "cells"> = { nbformat: 4, nbformatMinor: 5, metadata: {}, language: "python", hasIds: true };

export class NotebookController {
  readonly path: string;
  readonly state: NotebookState;
  private readonly set: SetStoreFunction<NotebookState>;
  /** The text the notebook was read from (cells' `origin` refers to it). */
  private base = "";
  /** The last text this editor put into the tab. */
  private written: string | null = null;
  private saving = false;
  private again = false;
  private unlisten: (() => void) | null = null;
  private starting: Promise<string | null> | null = null;
  readonly views = new Map<string, EditorView>();
  /** The cells' container (command mode focus). */
  container: HTMLElement | null = null;

  constructor(path: string) {
    this.path = path;
    const [state, set] = createStore<NotebookState>({
      loaded: false,
      error: null,
      meta: EMPTY_META,
      cells: [],
      selected: 0,
      mode: "command",
      kernel: { id: null, label: "", status: "none", server: false },
      showVariables: false,
      vars: [],
      children: {},
      expanded: {},
    });
    this.state = state;
    this.set = set;
  }

  /** Whether `text` (the tab's content) is what this editor wrote last. */
  isOwnText(text: string): boolean {
    return text === this.written;
  }

  /** Read the notebook from the tab's text. */
  async load(text: string): Promise<void> {
    try {
      const view = await jupyterApi.open(text);
      const { cells, ...meta } = view.notebook;
      this.base = text;
      this.written = text;
      batch(() => {
        this.set({
          loaded: true,
          error: null,
          meta,
          cells: cells.map((c, i) => toCellState(c, view.rendered[i] ?? [])),
          selected: Math.min(this.state.selected, Math.max(0, cells.length - 1)),
          mode: "command",
        });
      });
      this.views.clear();
    } catch (e) {
      this.set({ loaded: true, error: String(e).replace(/^Error: /, "") });
    }
  }

  /** Write the model into the tab (serialized in Rust, coalescing bursts of edits). */
  async sync(): Promise<void> {
    if (this.saving) {
      this.again = true;
      return;
    }
    this.saving = true;
    try {
      do {
        this.again = false;
        const nb = toNotebook(this.state.meta, this.state.cells);
        const text = await jupyterApi.serialize(nb, this.base);
        const tab = editor.tabs.find((t) => t.path === this.path);
        if (tab && tab.content !== text) {
          this.written = text;
          updateContent(this.path, text);
        } else this.written = text;
      } while (this.again);
    } catch (e) {
      console.warn("cannot serialize the notebook", e);
    } finally {
      this.saving = false;
    }
  }

  private update(fn: (cells: CellState[]) => void): void {
    this.set("cells", produce(fn));
    void this.sync();
  }

  cellIndex(id: string): number {
    return this.state.cells.findIndex((c) => c.id === id);
  }

  setSource(id: string, source: string): void {
    const i = this.cellIndex(id);
    if (i < 0 || this.state.cells[i].source === source) return;
    this.set("cells", i, "source", source);
    void this.sync();
  }

  // ---- Selection and modes ------------------------------------------------------------

  select(index: number, mode: "command" | "edit" = "command"): void {
    if (!this.state.cells.length) return;
    const i = Math.max(0, Math.min(index, this.state.cells.length - 1));
    this.set({ selected: i, mode });
    if (mode === "command") {
      queueMicrotask(() => {
        this.container?.focus({ preventScroll: true });
        this.container?.querySelector(`[data-cell-index="${i}"]`)?.scrollIntoView({ block: "nearest" });
      });
    }
  }

  enterEdit(index = this.state.selected): void {
    const cell = this.state.cells[index];
    if (!cell) return;
    if (cell.cellType !== "code") this.set("cells", index, "editing", true);
    this.set({ selected: index, mode: "edit" });
    queueMicrotask(() => this.views.get(cell.id)?.focus());
  }

  /** A cell editor got the focus (a click): edit mode on that cell. */
  editorFocused(index: number): void {
    if (this.state.selected !== index || this.state.mode !== "edit") this.set({ selected: index, mode: "edit" });
  }

  enterCommand(): void {
    const cell = this.state.cells[this.state.selected];
    this.set({ mode: "command" });
    if (cell && cell.cellType === "markdown" && cell.editing && cell.source.trim()) {
      this.set("cells", this.state.selected, "editing", false);
    }
    this.select(this.state.selected);
  }

  // ---- Cells --------------------------------------------------------------------------

  insert(at: number, cellType: CellType = "code", edit = true): void {
    const cell = newCell(
      cellType,
      this.state.cells.map((c) => c.id),
    );
    const index = Math.max(0, Math.min(at, this.state.cells.length));
    this.update((cells) => void cells.splice(index, 0, cell));
    if (edit) this.enterEdit(index);
    else this.select(index);
  }

  remove(index = this.state.selected): void {
    if (!this.state.cells[index]) return;
    const next = selectionAfterRemove(index, this.state.cells.length);
    this.update((cells) => void cells.splice(index, 1));
    this.select(next);
  }

  move(delta: number, index = this.state.selected): void {
    const to = moveTarget(index, delta, this.state.cells.length);
    if (to === null) return;
    this.update((cells) => {
      const [c] = cells.splice(index, 1);
      cells.splice(to, 0, c);
    });
    this.select(to, this.state.mode);
  }

  setType(cellType: CellType, index = this.state.selected): void {
    const cell = this.state.cells[index];
    if (!cell || cell.cellType === cellType) return;
    this.update((cells) => {
      const c = cells[index];
      c.cellType = cellType;
      c.editing = cellType !== "code" && !c.source.trim();
      if (cellType !== "code") {
        c.outputs = [];
        c.rendered = [];
        c.executionCount = null;
      }
    });
  }

  clearOutputs(index?: number): void {
    this.update((cells) => {
      cells.forEach((c, i) => {
        if (index !== undefined && i !== index) return;
        c.outputs = [];
        c.rendered = [];
        c.status = undefined;
        if (index === undefined) c.executionCount = null;
      });
    });
  }

  // ---- Running ------------------------------------------------------------------------

  /** Run a cell (Markdown cells render); then select the next one or insert a new one. */
  async run(index = this.state.selected, after: "stay" | "next" | "insert" = "stay"): Promise<void> {
    const cell = this.state.cells[index];
    if (!cell) return;
    if (cell.cellType === "code") void this.execute([index]);
    else this.set("cells", index, "editing", false);
    if (after === "insert") this.insert(index + 1, "code");
    else if (after === "next") {
      if (index + 1 >= this.state.cells.length) this.insert(index + 1, "code");
      else this.select(index + 1);
    } else this.select(index);
  }

  async runAll(): Promise<void> {
    this.state.cells.forEach((c, i) => {
      if (c.cellType === "markdown") this.set("cells", i, "editing", false);
    });
    const code = this.state.cells.flatMap((c, i) => (c.cellType === "code" && c.source.trim() ? [i] : []));
    await this.execute(code);
  }

  private async execute(indexes: number[]): Promise<void> {
    if (!indexes.length) return;
    const ids = indexes.map((i) => this.state.cells[i].id);
    // Mark them first so the toolbar shows the queue while the kernel starts.
    for (const i of indexes) this.set("cells", i, { running: true, started: false, status: undefined });
    const kernel = await this.ensureKernel();
    if (!kernel) {
      for (const id of ids) this.finish(id);
      return;
    }
    for (const id of ids) {
      const i = this.cellIndex(id);
      if (i < 0) continue;
      const cell = this.state.cells[i];
      this.set("cells", i, { outputs: [], rendered: [], clearPending: false, displayIds: {} });
      try {
        await jupyterApi.execute(kernel, id, cell.source);
      } catch (e) {
        this.finish(id);
        showJupyterBalloon("Cannot run the cell", String(e).replace(/^Error: /, ""), 8000, undefined, { type: "error" });
        return;
      }
    }
    void this.sync();
  }

  private finish(id: string): void {
    const i = this.cellIndex(id);
    if (i >= 0) this.set("cells", i, { running: false, started: false });
  }

  // ---- Kernel -------------------------------------------------------------------------

  /** The running kernel, started when needed (ipykernel installed with consent). */
  async ensureKernel(): Promise<string | null> {
    const k = this.state.kernel;
    if (k.id && k.status !== "dead") return k.id;
    this.starting ??= this.startKernel().finally(() => (this.starting = null));
    return this.starting;
  }

  private async startKernel(): Promise<string | null> {
    const server = getSetting<string>("jupyter.server.url", "").trim();
    if (!server) {
      const st = await jupyterApi.kernelStatus().catch((e) => {
        this.failed(String(e));
        return null;
      });
      if (!st) return null;
      if (st.needsServer) {
        this.failed(t("langs.jupyter.kernelsOfRunOnAJupyterServer", { interpreter: st.interpreter }));
        showJupyterBalloon(t("langs.jupyter.configureAJupyterServer"), t("langs.jupyter.forgeCannotStartKernelsOf", { interpreter: st.interpreter }), 10000, [
          { label: t("common.configureEllipsis"), run: () => openSettings("lang.jupyter") },
        ]);
        return null;
      }
      if (!st.installed && !(await installIpykernel(st.interpreter))) {
        this.failed(t("langs.jupyter.ipykernelIsNotInstalledIn", { interpreter: st.interpreter }));
        return null;
      }
    }
    this.set("kernel", { status: "starting", error: undefined, label: this.state.kernel.label || t("langs.jupyter.kernel") });
    try {
      const started = await jupyterApi.start({ path: this.path });
      this.unlisten?.();
      this.unlisten = await jupyterApi.onEvent(started.id, (e) => this.onEvent(e));
      this.set("kernel", { id: started.id, label: started.label, server: started.server, status: "idle" });
      return started.id;
    } catch (e) {
      this.failed(String(e));
      showJupyterBalloon(t("langs.jupyter.cannotStartTheKernel"), String(e).replace(/^Error: /, ""), 10000, undefined, { type: "error" });
      return null;
    }
  }

  private failed(error: string): void {
    this.set("kernel", { status: "failed", error: error.replace(/^Error: /, "") });
  }

  async interrupt(): Promise<void> {
    const id = this.state.kernel.id;
    if (id) await jupyterApi.interrupt(id).catch((e) => showJupyterBalloon("Cannot interrupt the kernel", String(e), 6000));
  }

  async restart(): Promise<void> {
    const id = this.state.kernel.id;
    if (!id) {
      await this.ensureKernel();
      return;
    }
    this.stopRunning();
    this.set("kernel", "status", "restarting");
    try {
      const started = await jupyterApi.restart(id);
      this.set("kernel", { label: started.label, status: "idle" });
      this.set({ vars: [], children: {}, expanded: {} });
      if (this.state.showVariables) void this.refreshVariables();
    } catch (e) {
      this.failed(String(e));
    }
  }

  async shutdown(): Promise<void> {
    const id = this.state.kernel.id;
    this.unlisten?.();
    this.unlisten = null;
    this.stopRunning();
    this.set("kernel", { id: null, status: "none" });
    this.set({ vars: [], children: {}, expanded: {} });
    if (id) await jupyterApi.shutdown(id).catch(() => undefined);
  }

  private stopRunning(): void {
    this.state.cells.forEach((c, i) => c.running && this.set("cells", i, { running: false, started: false }));
  }

  private onEvent(e: KernelEvent): void {
    switch (e.type) {
      case "status":
        this.set("kernel", "status", e.state as KernelStatus);
        return;
      case "disconnected":
      case "exited":
        this.stopRunning();
        this.set("kernel", { status: "dead", id: this.state.kernel.status === "restarting" ? this.state.kernel.id : null });
        return;
      case "updateDisplay":
        this.state.cells.forEach((c, i) => {
          for (const [k, d] of Object.entries(c.displayIds ?? {})) {
            if (d !== e.displayId) continue;
            const at = Number(k);
            this.set("cells", i, "outputs", at, e.output);
            this.set("cells", i, "rendered", at, e.rendered);
          }
        });
        void this.sync();
        return;
    }
    const i = this.cellIndex(e.cellId);
    if (i < 0) return;
    switch (e.type) {
      case "executeInput":
        this.set("cells", i, { started: true, executionCount: e.executionCount });
        break;
      case "output":
        this.addOutput(i, e.output, e.rendered, e.displayId);
        break;
      case "clearOutput":
        if (e.wait) this.set("cells", i, "clearPending", true);
        else this.set("cells", i, { outputs: [], rendered: [], displayIds: {} });
        break;
      case "done":
        this.set("cells", i, { running: false, started: false, status: e.status, executionCount: e.executionCount ?? this.state.cells[i].executionCount });
        if (e.status !== "ok") {
          // The kernel skips the rest of the queue after an error.
          this.state.cells.forEach((c, j) => j !== i && c.running && !c.started && this.set("cells", j, { running: false, status: "aborted" }));
        }
        if (this.state.showVariables) void this.refreshVariables();
        break;
    }
    void this.sync();
  }

  private addOutput(i: number, output: Output, rendered: Rendered, displayId?: string): void {
    this.set(
      "cells",
      i,
      produce((c) => {
        if (c.clearPending) {
          c.outputs = [];
          c.rendered = [];
          c.displayIds = {};
          c.clearPending = false;
        }
        // Consecutive stream text of the same stream continues the last output.
        const last = c.outputs[c.outputs.length - 1];
        if (output.outputType === "stream" && last?.outputType === "stream" && last.name === output.name) {
          last.text += output.text;
          void jupyterApi.render([last]).then((r) => {
            const at = this.state.cells[i]?.outputs.indexOf(last);
            if (r[0] && at !== undefined && at >= 0) this.set("cells", i, "rendered", at, r[0]);
          });
          return;
        }
        c.outputs.push(output);
        c.rendered.push(rendered);
        if (displayId) c.displayIds = { ...(c.displayIds ?? {}), [c.outputs.length - 1]: displayId };
      }),
    );
  }

  // ---- Variables ----------------------------------------------------------------------

  toggleVariables(): void {
    const show = !this.state.showVariables;
    this.set("showVariables", show);
    if (show) void this.refreshVariables();
  }

  async refreshVariables(): Promise<void> {
    const id = this.state.kernel.id;
    if (!id || this.state.kernel.status === "starting") return;
    try {
      const vars = await jupyterApi.variables(id);
      this.set({ vars });
      for (const expr of Object.keys(this.state.expanded)) {
        if (this.state.expanded[expr]) void this.loadChildren(expr);
      }
    } catch (e) {
      console.warn("variables", e);
    }
  }

  async toggleExpanded(v: JupyterVariable): Promise<void> {
    const open = !this.state.expanded[v.expr];
    this.set("expanded", v.expr, open);
    if (open) await this.loadChildren(v.expr);
  }

  private async loadChildren(expr: string): Promise<void> {
    const id = this.state.kernel.id;
    if (!id) return;
    const kids = await jupyterApi.children(id, expr).catch(() => []);
    this.set("children", expr, kids);
  }

  dispose(): void {
    this.unlisten?.();
    this.unlisten = null;
    if (this.state.kernel.id && getSetting<boolean>("jupyter.kernel.shutdownOnClose", true) !== false) void this.shutdown();
  }
}

/** ipykernel in the interpreter with the user's consent (or without asking when set). */
export async function installIpykernel(interpreter: string): Promise<boolean> {
  if (!getSetting<boolean>("jupyter.kernel.installWithoutAsking", false)) {
    const answer = await askConfirmWithCheckbox({
      title: "Install ipykernel",
      message: `Running notebook cells needs ipykernel (BSD licence) in ${interpreter}. Install it now with the environment's package manager?`,
      confirm: "Install",
      checkbox: "Install without asking next time",
    });
    if (!answer.ok) return false;
    if (answer.checked) await saveSettings({ "jupyter.kernel.installWithoutAsking": true });
  }
  const cmd = await pythonApi.packagesCommand("install", ["ipykernel"]);
  const code = await runCommandLine(commandName(cmd), cmd);
  if (code !== 0) {
    showJupyterBalloon("ipykernel was not installed", "See the Run tool window for the package manager's output.", 8000);
    return false;
  }
  return true;
}

const controllers = new Map<string, NotebookController>();

/** The controller of an open notebook (one per file, shared by split editors). */
export function notebookFor(path: string): NotebookController {
  let c = controllers.get(path);
  if (!c) {
    c = new NotebookController(path);
    controllers.set(path, c);
  }
  return c;
}

export function existingNotebook(path: string | null | undefined): NotebookController | undefined {
  return path ? controllers.get(path) : undefined;
}

export function releaseNotebook(path: string): void {
  controllers.get(path)?.dispose();
  controllers.delete(path);
}
