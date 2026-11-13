import { describe, expect, it } from "vitest";
import { invoke, listen } from "@/ipc";
import { runEvent, type CommandLine, type RunConfiguration, type RunEvent, type TestNodeData } from "@/ipc/run";
import type { TestFile } from "@/ipc/testFrameworks";
import type { DebugEvent, SessionInfo } from "@/features/debug/api";
import { webDemos } from "./demoWebProjects";
import "./runMock";
import "./testFrameworksMock";
import "./debugMock";
import "./dapMock";
import "./jsDebugMock";

const files = webDemos.node.files;

async function run(runId: string, configuration: RunConfiguration, ms = 1500): Promise<RunEvent[]> {
  const events: RunEvent[] = [];
  const off = await listen<RunEvent>(runEvent(runId), (e) => events.push(e));
  await invoke("run_start", { request: { runId, configuration, testRunner: true } });
  await new Promise((r) => setTimeout(r, ms));
  off();
  return events;
}

const testsOf = (events: RunEvent[]) => {
  const byId = new Map<number, TestNodeData>();
  for (const e of events) if (e.type === "tests") for (const n of e.nodes) byId.set(n.id, n);
  return [...byId.values()].filter((n) => n.kind === "test");
};

describe("Node.js test runners (webforge-js-testing through WebAssembly)", () => {
  it("runs Vitest, Jest and Node.js configurations with coverage", async () => {
    const vitest: RunConfiguration = { name: "unit", type: "vitest", options: { scope: "all", coverageProvider: "istanbul" } };
    const v = await invoke<CommandLine>("run_command_line", { request: { configuration: vitest, coverage: true } });
    expect(v.args).toEqual(expect.arrayContaining(["--coverage.enabled=true", "--coverage.reporter=json", "--coverage.provider=istanbul"]));
    const jest: RunConfiguration = { name: "jest", type: "jest", options: { scope: "all", coverageProvider: "v8" } };
    const j = await invoke<CommandLine>("run_command_line", { request: { configuration: jest, coverage: true } });
    expect(j.args).toEqual(expect.arrayContaining(["--coverageReporters=json", "--coverageProvider=v8"]));
    const node: RunConfiguration = { name: "server", type: "node", options: { file: "src/server.ts" } };
    const n = await invoke<CommandLine>("run_command_line", { request: { configuration: node, coverage: true } });
    expect(n.args.slice(0, 3)).toEqual(["node_modules/c8/bin/c8.js", "--reporter=lcov", "--reports-dir=/tmp/forge-test-reports/run-coverage"]);
    expect(n.args).toContain("src/server.ts");
    const mocha: RunConfiguration = { name: "mocha", type: "mocha", options: { scope: "all", coverageTool: "nyc" } };
    const m = await invoke<CommandLine>("run_command_line", { request: { configuration: mocha, coverage: true } });
    expect(m.args[0]).toBe("node_modules/nyc/bin/nyc.js");
  });

  it("discovers Vitest and Playwright tests of the Node.js demo", async () => {
    const unit = await invoke<TestFile>("test_discover", { path: "src/orders/pricing.test.ts", text: files["src/orders/pricing.test.ts"] });
    expect(unit.framework).toBe("vitest");
    expect(unit.targets.map((t) => [t.line, t.kind, t.label])).toEqual([
      [6, "describe", "pricing"],
      [7, "test", "multiplies the unit price"],
      [11, "describe", "orderTotal"],
      [12, "test", "adds the tax"],
      [16, "test", "applies the voucher before the tax"],
      [21, "test", "rounds to cents"],
    ]);
    expect(unit.targets[4].configuration.options).toMatchObject({ scope: "test", test: "pricing orderTotal applies the voucher before the tax" });
    const e2e = await invoke<TestFile>("test_discover", { path: "e2e/checkout.spec.ts", text: files["e2e/checkout.spec.ts"] });
    expect(e2e.framework).toBe("playwright");
    expect(e2e.targets.map((t) => t.label)).toEqual(["reports the health", "order totals", "returns the total of an order", "answers 404 for unknown orders"]);
  });

  it("runs Vitest with Node.js into the test tree and reruns the failed test in its file", { timeout: 15000 }, async () => {
    const cfg: RunConfiguration = { name: "Unit tests", type: "vitest", options: { scope: "all" } };
    const line = await invoke<CommandLine>("run_command_line", { request: { configuration: cfg } });
    expect([line.program, ...line.args].slice(0, 3)).toEqual(["node", "node_modules/vitest/vitest.mjs", "run"]);
    const tests = testsOf(await run("vitest-1", cfg));
    expect(tests.map((t) => [t.name, t.status])).toEqual([
      ["multiplies the unit price", "passed"],
      ["adds the tax", "passed"],
      ["applies the voucher before the tax", "failed"],
      ["rounds to cents", "passed"],
      ["finds an order by id", "passed"],
      ["returns undefined for unknown ids", "passed"],
    ]);
    const failed = tests.find((t) => t.status === "failed")!;
    expect([failed.expected, failed.actual]).toEqual(["64.24", "64.23"]);
    expect(failed.location).toMatchObject({ workspacePath: "src/orders/pricing.test.ts", line: 17 });
    const rerun = await invoke<RunConfiguration>("test_rerun_failed", {
      configuration: cfg,
      failed: [{ name: failed.name, path: "src/orders/pricing.test.ts" }],
    });
    expect(rerun.options).toMatchObject({ scope: "pattern", pattern: "applies the voucher before the tax", files: "src/orders/pricing.test.ts" });
    const again = testsOf(await run("vitest-2", rerun));
    expect(again.map((t) => t.name)).toEqual(["applies the voucher before the tax"]);
  });

  it("runs Playwright with its JSON report", async () => {
    const tests = testsOf(await run("pw-1", { name: "E2E", type: "playwright", options: { scope: "all" } }));
    expect(tests.map((t) => [t.name, t.status])).toEqual([
      ["reports the health", "passed"],
      ["returns the total of an order", "failed"],
      ["answers 404 for unknown orders", "passed"],
    ]);
  });
});

describe("JavaScript debugger mock", () => {
  it("starts js-debug's child session and stops at a TypeScript breakpoint", async () => {
    await invoke("debug_breakpoint_add", { spec: { file: "src/orders/pricing.ts", line: 12 } });
    const events: SessionInfo[] = [];
    const off = await listen<DebugEvent>("debug://session", (e) => e.type === "session" && events.push(e.session));
    const request = await invoke<{ adapter: string }>("js_debug_request", { configuration: { name: "server", type: "node", options: { file: "src/server.ts" } } });
    expect(request.adapter).toBe("pwa-node");
    const parent = await invoke<SessionInfo>("dap_start", { request });
    await new Promise((r) => setTimeout(r, 900));
    off();
    const paused = events.find((s) => s.status === "paused")!;
    expect(paused.parentId).toBe(parent.id);
    expect(paused.location).toMatchObject({ file: "src/orders/pricing.ts", line: 12 });
    const vars = await invoke<{ name: string; value: string }[]>("debug_variables", { sessionId: paused.id, depth: 0, context: 0 });
    expect(vars.map((v) => [v.name, v.value])).toContainEqual(["discount", "5.997"]);
    await invoke("debug_stop", { sessionId: paused.id });
  });
});
