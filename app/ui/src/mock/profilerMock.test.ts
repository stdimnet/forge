import { describe, expect, it } from "vitest";
import type { RecentSnapshot, Snapshot } from "@/ipc/profiler";
import type { CommandLine, RunEvent } from "@/ipc/run";

const { invoke, listen } = await import("@/ipc");
await import("./settingsMock");
await import("./runMock");
await import("./profilerMock");
const { demoKind } = await import("./profilerMock");

describe("profiler mock (forge-profiler through WebAssembly)", () => {
  it("opens the demo Xdebug snapshot with statistics and callers", async () => {
    const s = await invoke<Snapshot>("profiler_open", { path: "/tmp/cachegrind.out.4242.gz" });
    expect(s.kind).toBe("xdebug");
    expect(s.metrics.map((m) => m.kind)).toEqual(["Time", "Memory"]);
    const execute = s.functions.findIndex((f) => f.name === "php::PDOStatement->execute");
    expect(s.functions[execute].calls).toBe(2);
    const callers = s.edges.filter((e) => e.callee === execute).map((e) => s.functions[e.caller].name);
    expect(callers).toContain("App\\Reports\\SalesReport->topCustomers");
    expect(s.view?.tree.children[0].name).toBe("{main}");
    const memory = await invoke<Snapshot>("profiler_open", { path: "/tmp/cachegrind.out.4242.gz", metric: 1 });
    expect(memory.metric).toBe(1);
  });

  it("opens SPX, V8 CPU and heap snapshots", async () => {
    expect((await invoke<Snapshot>("profiler_open", { path: "spx-full-1.json" })).title).toBe("php artisan shop:report");
    const cpu = await invoke<Snapshot>("profiler_open", { path: "CPU.1.cpuprofile" });
    expect(cpu.callsLabel).toBe("Samples");
    const heap = await invoke<Snapshot>("profiler_open", { path: "Heap.1.heapsnapshot" });
    expect(heap.heap?.classes.find((c) => c.name === "Order")?.count).toBe(400);
    await expect(invoke("profiler_open", { path: "notes.txt" })).rejects.toThrow(/not a profiler snapshot/);
    const recent = await invoke<RecentSnapshot[]>("profiler_recent");
    expect(recent.map((r) => demoKind(r.path))).toEqual(["xdebug", "spx", "v8-cpu", "v8-heap-sampling"]);
  });

  it("profiles a PHP run and refuses other configurations", async () => {
    const configuration = { name: "report", type: "php-script", options: { file: "bin/report.php" } };
    const cmd = await invoke<CommandLine>("run_command_line", { request: { runId: "p1", configuration, profiler: "xdebug" } });
    expect(cmd.args.slice(0, 2)).toEqual(["-dxdebug.mode=profile", "-dxdebug.start_with_request=yes"]);
    expect(cmd.env?.XDEBUG_MODE).toBe("profile");
    const events: RunEvent[] = [];
    const done = new Promise<void>((resolve) => {
      void listen<RunEvent>("run://p2", (e) => {
        events.push(e);
        if (e.type === "exited") resolve();
      });
    });
    await invoke("run_start", { request: { runId: "p2", configuration, profiler: "spx" } });
    await done;
    const profiled = events.find((e) => e.type === "profiler");
    expect(profiled?.type === "profiler" && profiled.snapshot?.kind).toBe("spx");
    await expect(invoke("run_start", { request: { runId: "p3", configuration: { name: "n", type: "npm", options: {} }, profiler: "xdebug" } })).rejects.toThrow(/PHP configuration/);
  });
});
