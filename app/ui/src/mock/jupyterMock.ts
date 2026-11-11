/**
 * Browser-mode Jupyter: notebooks, outputs, diffs and DataFrame pages run the Rust code of
 * `pyforge-jupyter` as WebAssembly; the kernel is simulated for the demo notebook
 * (`notebooks/sales.ipynb` of `?project=python`): it streams the same messages a kernel
 * sends (status, execute_input, outputs, the reply) for the demo's cells, keeps the
 * variables a cell assigns and serves View as DataFrame for `sales`.
 */
import { emitMock, registerMock } from "@/ipc";
import { callWasm, forgeWasm } from "@/wasm";
import type { DataFrame, JupyterVariable, KernelEvent, Output, Rendered } from "@/ipc/jupyter";
import { salesNotebook } from "./demoNotebooks";

const ROOT = "/home/dev/Projects/shop-api";

registerMock("jupyter_open", async ({ text }) => callWasm((await forgeWasm()).jupyter_open, { text, root: ROOT }));
registerMock("jupyter_serialize", async ({ notebook, original }) => callWasm((await forgeWasm()).jupyter_serialize, { notebook, original }));
registerMock("jupyter_render", async ({ outputs }) => callWasm((await forgeWasm()).jupyter_render, { outputs, root: ROOT }));
registerMock("jupyter_diff", async ({ left, right }) => callWasm((await forgeWasm()).jupyter_diff, { left, right }));
registerMock("jupyter_dataframe_query", async ({ frame, query }) => callWasm((await forgeWasm()).jupyter_dataframe_query, { frame, query }));

registerMock("jupyter_kernel_status", () => ({ installed: true, version: "6.29.5", interpreter: "Python 3.12 (shop-api)", needsServer: false }));

/** Outputs of the demo notebook's cells, by a piece of their code. */
function demoOutputs(): { match: RegExp; outputs: Output[] }[] {
  const nb = JSON.parse(salesNotebook) as { cells: { source: string[]; outputs?: unknown[] }[] };
  const join = (v: unknown) => (Array.isArray(v) ? v.join("") : String(v ?? ""));
  const toOutput = (raw: Record<string, unknown>): Output => {
    const data = Object.fromEntries(Object.entries((raw.data ?? {}) as Record<string, unknown>).map(([k, v]) => [k, k.endsWith("json") ? v : join(v)]));
    switch (raw.output_type) {
      case "stream":
        return { outputType: "stream", name: String(raw.name), text: join(raw.text) };
      case "execute_result":
        return { outputType: "execute_result", executionCount: null, data, metadata: {} };
      case "error":
        return { outputType: "error", ename: String(raw.ename), evalue: String(raw.evalue), traceback: raw.traceback as string[] };
      default:
        return { outputType: "display_data", data, metadata: {} };
    }
  };
  const of = (i: number) => ((nb.cells[i]?.outputs ?? []) as Record<string, unknown>[]).map(toOutput);
  return [
    { match: /regions loaded/, outputs: of(1) },
    { match: /plt\.show\(\)|\.plot\(/, outputs: of(4) },
    { match: /\["profit"\]|\.profit\b/, outputs: of(6) },
    { match: /\["revenue"\]\.sum\(\)/, outputs: of(5) },
    { match: /^\s*(sales|sales\.head\(\d*\)|df)\s*$/m, outputs: of(2) },
  ];
}

const SALES: DataFrame = {
  columns: ["id", "region", "orders", "revenue", "avg_basket"],
  dtypes: ["", "object", "int64", "float64", "float64"],
  indexColumns: 1,
  rows: [
    ["0", "North", "1284", "48210.5", "37.55"],
    ["1", "South", "932", "35120.0", "37.68"],
    ["2", "East", "1511", "61980.25", "41.02"],
    ["3", "West", "768", "27455.75", "35.75"],
    ["4", "Central", "1043", "40112.0", "38.46"],
  ],
  totalRows: 5,
  totalColumns: 4,
};

interface Kernel {
  count: number;
  vars: Map<string, JupyterVariable>;
  timers: number[];
  running: string | null;
  /** When the queued cells are done (ms since epoch). */
  busyUntil: number;
}

const kernels = new Map<string, Kernel>();
let next = 0;

const emit = (id: string, e: KernelEvent) => emitMock(`jupyter://${id}`, e);

function variable(name: string, type: string, value: string, extra: Partial<JupyterVariable> = {}): JupyterVariable {
  return { name, type, value, expr: name, children: false, size: null, ...extra };
}

/** Variables a cell defines (`name = …` at the start of a line), with demo values. */
function assign(k: Kernel, code: string): void {
  for (const m of code.matchAll(/^([A-Za-z_]\w*)(?:\s*,\s*([A-Za-z_]\w*))?\s*=(?!=)/gm)) {
    for (const name of [m[1], m[2]].filter(Boolean) as string[]) {
      if (name === "sales") k.vars.set("sales", variable("sales", "pandas.core.frame.DataFrame", "DataFrame 5×4", { frame: true }));
      else if (name === "fig") k.vars.set("fig", variable("fig", "matplotlib.figure.Figure", "Figure(600x300)", { children: true }));
      else if (name === "ax") k.vars.set("ax", variable("ax", "matplotlib.axes._axes.Axes", "Axes(0.125,0.11;0.775x0.77)", { children: true }));
      else k.vars.set(name, variable(name, "int", "42"));
    }
  }
}

registerMock("jupyter_kernel_start", ({ request }) => {
  const id = (request as { id?: string } | undefined)?.id || `kernel-${++next}`;
  kernels.set(id, { count: 0, vars: new Map(), timers: [], running: null, busyUntil: 0 });
  setTimeout(() => emit(id, { type: "status", state: "idle" }), 50);
  return { id, label: "Python 3.12 (shop-api)", server: false };
});

registerMock("jupyter_execute", async ({ id, cellId, code }) => {
  const k = kernels.get(String(id));
  if (!k) throw new Error(`The kernel ${id} is not running`);
  const kid = String(id);
  const cid = String(cellId);
  const src = String(code);
  const known = demoOutputs().find((d) => d.match.test(src));
  let outputs: Output[] = known?.outputs ?? [];
  if (!known && /^\s*1\s*\/\s*0\s*$/m.test(src)) {
    outputs = [
      {
        outputType: "error",
        ename: "ZeroDivisionError",
        evalue: "division by zero",
        traceback: [
          "\u001b[0;31m---------------------------------------------------------------------------\u001b[0m",
          "\u001b[0;31mZeroDivisionError\u001b[0m                         Traceback (most recent call last)",
          "Cell \u001b[0;32mIn[1], line 1\u001b[0m\n\u001b[0;32m----> 1\u001b[0m \u001b[38;5;241;43m1\u001b[39;49m\u001b[38;5;241;43m/\u001b[39;49m\u001b[38;5;241;43m0\u001b[39;49m\n",
          "\u001b[0;31mZeroDivisionError\u001b[0m: division by zero",
        ],
      },
    ];
  } else if (!known) {
    const printed = [...src.matchAll(/print\((["'])(.*?)\1\)/g)].map((m) => `${m[2]}\n`).join("");
    if (printed) outputs = [{ outputType: "stream", name: "stdout", text: printed }];
  }
  const rendered = outputs.length ? await callWasm<Rendered[]>((await forgeWasm()).jupyter_render, { outputs, root: ROOT }) : [];
  const failed = outputs.some((o) => o.outputType === "error");
  // Cells run one after another, like a kernel's queue.
  const duration = 300 + outputs.length * 120;
  const start = Math.max(Date.now(), k.busyUntil);
  k.busyUntil = start + duration + 20;
  const at = (ms: number, f: () => void) => k.timers.push(window.setTimeout(f, start - Date.now() + ms));
  let count = 0;
  at(0, () => {
    k.running = cid;
    emit(kid, { type: "status", state: "busy" });
    count = ++k.count;
    emit(kid, { type: "executeInput", cellId: cid, executionCount: count });
  });
  outputs.forEach((o, i) =>
    at(250 + i * 120, () => {
      const out = o.outputType === "execute_result" ? { ...o, executionCount: count } : o;
      emit(kid, { type: "output", cellId: cid, output: out, rendered: rendered[i] });
    }),
  );
  at(duration, () => {
    if (!failed) assign(k, src);
    emit(kid, { type: "done", cellId: cid, status: failed ? "error" : "ok", executionCount: count });
    emit(kid, { type: "status", state: "idle" });
    k.running = null;
  });
  return `msg-${k.count + 1}`;
});

registerMock("jupyter_interrupt", ({ id }) => {
  const k = kernels.get(String(id));
  if (!k?.running) return;
  const cid = k.running;
  k.timers.forEach(clearTimeout);
  k.timers = [];
  k.running = null;
  k.busyUntil = 0;
  emit(String(id), {
    type: "output",
    cellId: cid,
    output: { outputType: "error", ename: "KeyboardInterrupt", evalue: "", traceback: ["KeyboardInterrupt: "] },
    rendered: { kind: "text", error: true, lines: [{ segments: [{ text: "KeyboardInterrupt: ", style: { fg: "var(--ansi-red)" } }] }] },
  });
  emit(String(id), { type: "done", cellId: cid, status: "error", executionCount: k.count });
  emit(String(id), { type: "status", state: "idle" });
});

registerMock("jupyter_restart", ({ id }) => {
  const k = kernels.get(String(id));
  if (!k) throw new Error(`The kernel ${id} is not running`);
  k.timers.forEach(clearTimeout);
  kernels.set(String(id), { count: 0, vars: new Map(), timers: [], running: null, busyUntil: 0 });
  setTimeout(() => emit(String(id), { type: "status", state: "idle" }), 300);
  return { id, label: "Python 3.12 (shop-api)", server: false };
});

registerMock("jupyter_shutdown", ({ id }) => {
  kernels.get(String(id))?.timers.forEach(clearTimeout);
  kernels.delete(String(id));
});

registerMock("jupyter_variables", ({ id }) => [...(kernels.get(String(id))?.vars.values() ?? [])].sort((a, b) => a.name.localeCompare(b.name)));

registerMock("jupyter_variable_children", ({ expr }) =>
  String(expr) === "fig"
    ? [variable("dpi", "float", "100.0", { expr: "fig.dpi" }), variable("number", "int", "1", { expr: "fig.number" })]
    : [variable("title", "str", "''", { expr: `${expr}.title` })],
);

registerMock("jupyter_variable_frame", ({ expr }) => {
  if (String(expr) !== "sales") throw new Error(`NameError: name '${expr}' is not defined`);
  return SALES;
});

registerMock("jupyter_server_test", ({ url }) => {
  const u = String(url);
  if (!/^https?:\/\//.test(u)) throw new Error("Enter the server URL with http:// or https://");
  return { base: u.replace(/\/(lab|tree).*$/, "").replace(/\/$/, ""), defaultKernel: "python3", kernels: [{ name: "python3", displayName: "Python 3 (ipykernel)", language: "python" }] };
});
registerMock("jupyter_server_has_token", () => false);
registerMock("jupyter_server_forget_token", () => undefined);
