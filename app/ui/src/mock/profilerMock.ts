/**
 * Browser-mode profiler snapshots: `profiler_open` and `profiler_recent` answer with the
 * demo projects' Xdebug, SPX and V8 snapshots, generated in their real file formats and
 * parsed by the Rust crate `forge-profiler` through WebAssembly; runs with
 * `RunRequest.profiler` end with a `profiler` event like the desktop app's. Nothing is
 * read from or written to a real project.
 */
import { registerMock } from "@/ipc";
import type { RunEvent, RunRequest } from "@/ipc/run";
import type { RecentSnapshot, RunProfiler, Snapshot, SnapshotKind } from "@/ipc/profiler";
import { callWasmSync, forgeWasm } from "@/wasm";
import { mockCommandLine, projectRoot, registerMockRunHandler, startMockRun, type Step } from "./runMock";

const PHP_TYPES = new Set(["php-script", "phpunit", "pest", "codeception", "behat", "phpspec", "artisan", "php-builtin-server"]);
const CACHE = "~/.cache/forge/profiler";

/** The demo snapshot a path stands for. */
export function demoKind(path: string): SnapshotKind | null {
  const name = (path.split(/[\\/]/).pop() ?? "").toLowerCase();
  if (name.includes("cachegrind.out") || name.includes("callgrind.out")) return "xdebug";
  if (name.startsWith("spx-full-")) return "spx";
  if (name.endsWith(".cpuprofile")) return "v8-cpu";
  if (name.endsWith(".heapprofile")) return "v8-heap-sampling";
  if (name.endsWith(".heapsnapshot")) return "v8-heap";
  return null;
}

async function demo(kind: SnapshotKind, metric?: number | null): Promise<Snapshot> {
  const wasm = await forgeWasm();
  return callWasmSync<Snapshot>(wasm.profiler_demo, { kind, root: projectRoot(), metric: metric ?? null });
}

const FILES: Record<RunProfiler, string> = {
  xdebug: "1761646500000/cachegrind.out.4242.gz",
  spx: "1761646440000/spx-full-20261028_101500-acme-4242-1804289383.json",
  "v8-cpu": "1761646380000/CPU.20261028.101500.4242.0.001.cpuprofile",
  "v8-heap": "1761646320000/Heap.20261028.101500.4242.0.001.heapprofile",
};

registerMock("profiler_open", async ({ path, metric }) => {
  const kind = demoKind(String(path));
  if (!kind) throw new Error(`${String(path).split("/").pop()} is not a profiler snapshot (Xdebug cachegrind, SPX report, .cpuprofile, .heapprofile or .heapsnapshot)`);
  return demo(kind, metric as number | null);
});

registerMock("profiler_recent", (): RecentSnapshot[] =>
  Object.values(FILES).map((f) => ({ path: `${CACHE}/${f}`, name: f.split("/").pop()!, at: Number(f.split("/")[0]) })),
);

/** The flags a profiling run adds (as the Rust side does), or why it cannot run. */
function profiled(r: RunRequest, kind: RunProfiler): { args: string[]; env: Record<string, string> } {
  const type = r.configuration?.type ?? "";
  const dir = `/tmp/forge-test-reports/${r.runId ?? "run"}-profile`;
  if (kind === "xdebug" || kind === "spx") {
    if (!PHP_TYPES.has(type)) throw new Error(`'${type || "this"}' configurations cannot be profiled with ${kind === "xdebug" ? "Xdebug" : "SPX"}: choose a PHP configuration`);
    return kind === "xdebug"
      ? { args: ["-dxdebug.mode=profile", "-dxdebug.start_with_request=yes", `-dxdebug.output_dir=${dir}`], env: { XDEBUG_MODE: "profile" } }
      : { args: [`-dspx.data_dir=${dir}`], env: { SPX_ENABLED: "1", SPX_REPORT: "full", SPX_AUTO_START: "1" } };
  }
  const flags = kind === "v8-cpu" ? ["--cpu-prof", `--cpu-prof-dir=${dir}`] : ["--heap-prof", `--heap-prof-dir=${dir}`];
  if (type === "node") return { args: flags, env: {} };
  if (type === "npm") return { args: [], env: { NODE_OPTIONS: flags.join(" ") } };
  throw new Error(`'${type || "this"}' configurations cannot be profiled with V8: choose a Node.js or npm configuration`);
}

registerMockRunHandler({
  commandLine: async (r) => {
    if (!r.profiler || !r.configuration) return null;
    const cmd = mockCommandLine(r.configuration);
    const p = profiled(r, r.profiler);
    return { ...cmd, args: [...p.args, ...cmd.args], env: { ...cmd.env, ...p.env } };
  },
  start: async (r, runId) => {
    const kind = r.profiler;
    if (!kind || !r.configuration) return null;
    const cmd = mockCommandLine(r.configuration);
    const p = profiled({ ...r, runId }, kind);
    const php = kind === "xdebug" || kind === "spx";
    const steps: Step[] = php
      ? [[300, "Sales report for October 2026\n"], [200, "  12 customers, 400 orders, 41 380.20 EUR\n"], [100, { code: 0 }]]
      : [[300, "orders-api listening on http://localhost:3000\n"], [400, "GET /orders/42 200 18 ms\n"], [100, { code: 0 }]];
    const snapshotKind: SnapshotKind = kind === "v8-heap" ? "v8-heap-sampling" : kind;
    return startMockRun(runId, { ...cmd, args: [...p.args, ...cmd.args], env: { ...cmd.env, ...p.env } }, steps, {
      exitEvent: async (): Promise<RunEvent> => ({ type: "profiler", snapshot: await demo(snapshotKind), path: `${CACHE}/${FILES[kind]}` }),
    });
  },
});
