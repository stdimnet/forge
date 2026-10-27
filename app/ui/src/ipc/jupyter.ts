/**
 * Jupyter notebooks (`commands/jupyter.rs`, `packs/pyforge/jupyter`): the notebook model,
 * rendered outputs, DataFrame pages, notebook diffs and kernels. Kernel events stream on
 * `jupyter://<kernel id>`.
 */
import { invoke, listen } from "./index";
import type { ConsoleLine } from "./run";

export type CellType = "code" | "markdown" | "raw";

/** MIME type → value (texts joined, JSON types as JSON). */
export type MimeBundle = Record<string, unknown>;

export type Output =
  | { outputType: "stream"; name: string; text: string }
  | { outputType: "execute_result"; executionCount: number | null; data: MimeBundle; metadata: Record<string, unknown> }
  | { outputType: "display_data"; data: MimeBundle; metadata: Record<string, unknown> }
  | { outputType: "error"; ename: string; evalue: string; traceback: string[] };

export interface Cell {
  id: string;
  cellType: CellType;
  source: string;
  metadata: Record<string, unknown>;
  executionCount: number | null;
  outputs: Output[];
  attachments?: unknown;
  /** Position in the file the notebook was read from (dropped on copies). */
  origin?: number;
}

export interface Notebook {
  nbformat: number;
  nbformatMinor: number;
  metadata: Record<string, unknown>;
  cells: Cell[];
  language: string;
  hasIds: boolean;
}

export interface DataFrame {
  columns: string[];
  dtypes?: string[];
  indexColumns: number;
  rows: string[][];
  totalRows?: number;
  totalColumns?: number;
}

/** How an output is shown. */
export type Rendered =
  | { kind: "text"; lines: ConsoleLine[]; error: boolean }
  | { kind: "html"; html: string }
  | { kind: "image"; src: string; mime: string; width?: number; height?: number }
  | { kind: "json"; value: unknown }
  | { kind: "markdown"; text: string }
  | { kind: "dataFrame"; frame: DataFrame };

export interface NotebookView {
  notebook: Notebook;
  /** `rendered[cell][output]`. */
  rendered: Rendered[][];
}

export interface DataFrameQuery {
  sort?: { column: number; descending: boolean } | null;
  filter?: string;
  page?: number;
  /** 0 = all rows. */
  pageSize?: number;
}

export interface DataFramePage {
  rows: string[][];
  rowNumbers: number[];
  matched: number;
  page: number;
  pages: number;
}

export type DiffLineKind = "equal" | "insert" | "delete";

export interface DiffLine {
  kind: DiffLineKind;
  text: string;
  left?: number;
  right?: number;
}

export interface TextChange {
  changed: boolean;
  inserted: number;
  deleted: number;
  lines: DiffLine[];
}

export type CellStatus = "unchanged" | "modified" | "added" | "removed";

export interface CellSide {
  index: number;
  id: string;
  cellType: CellType;
  executionCount?: number;
}

export interface CellDiff {
  status: CellStatus;
  left?: CellSide;
  right?: CellSide;
  source: TextChange;
  outputs: TextChange;
  typeChanged: boolean;
  metadataChanged: boolean;
  executionCountChanged: boolean;
}

export interface NotebookDiff {
  cells: CellDiff[];
  metadataChanged: boolean;
  stats: { added: number; removed: number; modified: number; unchanged: number };
  language: string;
}

export interface IpykernelStatus {
  installed: boolean;
  version?: string | null;
  interpreter: string;
  /** Docker / SSH / WSL interpreters and remote projects need a Jupyter server. */
  needsServer: boolean;
}

export interface KernelRequest {
  /** The notebook (workspace-relative). */
  path: string;
  interpreter?: string;
  /** A Jupyter server URL (empty: the `jupyter.server.url` setting). */
  server?: string;
  kernelName?: string;
}

export interface KernelStarted {
  id: string;
  label: string;
  server: boolean;
}

/** A variable of the kernel's namespace (the Python Console's shape). */
export interface JupyterVariable {
  name: string;
  type: string;
  value: string;
  size?: number | null;
  expr: string;
  children: boolean;
  /** A DataFrame, Series or array: View as DataFrame. */
  frame?: boolean;
}

export type KernelEvent =
  | { type: "status"; state: string }
  | { type: "executeInput"; cellId: string; executionCount: number | null }
  | { type: "output"; cellId: string; output: Output; rendered: Rendered; displayId?: string }
  | { type: "updateDisplay"; displayId: string; output: Output; rendered: Rendered }
  | { type: "clearOutput"; cellId: string; wait: boolean }
  | { type: "done"; cellId: string; status: "ok" | "error" | "aborted"; executionCount: number | null }
  | { type: "disconnected" }
  | { type: "exited"; code: number | null };

export interface ServerCheck {
  base: string;
  defaultKernel: string;
  kernels: { name: string; displayName: string; language: string }[];
}

export const jupyterApi = {
  open: (text: string) => invoke<NotebookView>("jupyter_open", { text }),
  serialize: (notebook: Notebook, original?: string) => invoke<string>("jupyter_serialize", { notebook, original }),
  render: (outputs: Output[]) => invoke<Rendered[]>("jupyter_render", { outputs }),
  diff: (left: string, right: string) => invoke<NotebookDiff>("jupyter_diff", { left, right }),
  dataframeQuery: (frame: DataFrame, query: DataFrameQuery) => invoke<DataFramePage>("jupyter_dataframe_query", { frame, query }),
  kernelStatus: (interpreter?: string) => invoke<IpykernelStatus>("jupyter_kernel_status", { interpreter }),
  start: (request: KernelRequest) => invoke<KernelStarted>("jupyter_kernel_start", { request }),
  execute: (id: string, cellId: string, code: string) => invoke<string>("jupyter_execute", { id, cellId, code }),
  interrupt: (id: string) => invoke<void>("jupyter_interrupt", { id }),
  restart: (id: string) => invoke<KernelStarted>("jupyter_restart", { id }),
  shutdown: (id: string) => invoke<void>("jupyter_shutdown", { id }),
  variables: (id: string) => invoke<JupyterVariable[]>("jupyter_variables", { id }),
  children: (id: string, expr: string) => invoke<JupyterVariable[]>("jupyter_variable_children", { id, expr }),
  frame: (id: string, expr: string, limit?: number) => invoke<DataFrame>("jupyter_variable_frame", { id, expr, limit }),
  serverTest: (url: string, token?: string) => invoke<ServerCheck>("jupyter_server_test", { url, token }),
  serverHasToken: (url: string) => invoke<boolean>("jupyter_server_has_token", { url }),
  serverForgetToken: (url: string) => invoke<void>("jupyter_server_forget_token", { url }),
  onEvent: (id: string, cb: (e: KernelEvent) => void) => listen<KernelEvent>(`jupyter://${id}`, cb),
};
