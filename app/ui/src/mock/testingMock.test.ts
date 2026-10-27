import { describe, expect, it } from "vitest";
import { invoke, listen } from "@/ipc";
import { runEvent, type CommandLine, type RunConfiguration, type RunEvent, type TestNodeData } from "@/ipc/run";
import type { TestFile } from "@/ipc/testFrameworks";
import type { CoverageSuite, CoverageView, LineMark } from "@/ipc/testing";
import { luaDemoFiles } from "./demoLuaProject";
import "./runMock";
import "./testFrameworksMock";
import "./coverageMock";

async function run(runId: string, configuration: RunConfiguration, extra: object = {}, ms = 1200): Promise<RunEvent[]> {
  const events: RunEvent[] = [];
  const off = await listen<RunEvent>(runEvent(runId), (e) => events.push(e));
  await invoke("run_start", { request: { runId, configuration, testRunner: true, ...extra } });
  await new Promise((r) => setTimeout(r, ms));
  off();
  return events;
}

const nodesOf = (events: RunEvent[]) => {
  const byId = new Map<number, TestNodeData>();
  for (const e of events) if (e.type === "tests") for (const n of e.nodes) byId.set(n.id, n);
  return [...byId.values()];
};

describe("shared test runner mock (forge-testing through WebAssembly)", () => {
  it("discovers busted specs from the LuaForge manifest", async () => {
    const file = await invoke<TestFile>("test_discover", { path: "spec/entity_spec.lua", text: luaDemoFiles["spec/entity_spec.lua"] });
    expect(file.framework).toBe("busted");
    expect(file.targets.map((t) => [t.kind, t.label, t.key])).toEqual([
      ["describe", "Entity", ""],
      ["test", "starts with 10 hit points", "spec/entity_spec.lua::Entity starts with 10 hit points"],
      ["describe", "damage", ""],
      ["test", "never goes below zero", "spec/entity_spec.lua::Entity damage never goes below zero"],
      ["test", "kills at zero hit points", "spec/entity_spec.lua::Entity damage kills at zero hit points"],
    ]);
    expect(file.file?.options).toEqual({ scope: "file", file: "spec/entity_spec.lua" });
  });

  it("runs busted with TAP into the test tree and reruns the failed tests", async () => {
    const cfg: RunConfiguration = { name: "player_spec", type: "busted", options: { scope: "file", file: "spec/player_spec.lua" } };
    const line = await invoke<CommandLine>("run_command_line", { request: { configuration: cfg } });
    expect([line.program, ...line.args]).toEqual(["busted", "--output=TAP", "spec/player_spec.lua"]);
    const nodes = nodesOf(await run("busted-1", cfg));
    const rows = nodes.filter((n) => n.kind === "test").map((n) => [n.name, n.status]);
    expect(rows).toEqual([
      ["Player picks up gold", "passed"],
      ["Player moves by speed and frame time", "failed"],
      ["Player drops gold on death", "ignored"],
    ]);
    const failed = nodes.find((n) => n.status === "failed" && n.kind === "test")!;
    expect([failed.expected, failed.actual]).toEqual(["(number) 5", "(number) 4"]);
    expect(failed.location?.workspacePath).toBe("spec/player_spec.lua");
    const passed = nodes.find((n) => n.name === "Player picks up gold")!;
    expect(passed.location).toMatchObject({ workspacePath: "spec/player_spec.lua", member: "Player picks up gold" });
    const rerun = await invoke<RunConfiguration>("test_rerun_failed", { configuration: cfg, failed: [{ name: failed.name }] });
    expect(rerun.options).toMatchObject({ scope: "pattern", pattern: "Player moves by speed and frame time" });
  });

  it("runs PHPUnit with coverage and shows the suite", async () => {
    const cfg: RunConfiguration = { name: "All tests", type: "phpunit", options: { scope: "config" } };
    const line = await invoke<CommandLine>("run_command_line", { request: { configuration: cfg, coverage: true } });
    expect(line.args.slice(-2)).toEqual(["--coverage-clover", "/tmp/forge-test-reports/run-coverage/clover.xml"]);
    const events = await run("coverage-1", cfg, { coverage: true }, 2500);
    const cov = events.find((e) => e.type === "coverage") as Extract<RunEvent, { type: "coverage" }>;
    expect(cov.suite?.format).toBe("Clover");
    const exited = events.findIndex((e) => e.type === "exited");
    expect(events.indexOf(cov)).toBeLessThan(exited);
    const suites = await invoke<CoverageSuite[]>("coverage_suites");
    expect(suites[0].name).toBe("All tests");
    const view = await invoke<CoverageView>("coverage_view", {});
    expect(view.nodes[0].kind).toBe("project");
    expect(view.nodes.some((n) => n.path === "app/Models/User.php" && n.kind === "file")).toBe(true);
    const marks = await invoke<LineMark[]>("coverage_file", { path: "app/Models/User.php" });
    expect(marks.length).toBeGreaterThan(3);
    expect(new Set(marks.map((m) => m.status)).has("covered")).toBe(true);
    expect(view.hints["app/Models"]).toMatch(/^\d+% files, \d+% lines covered$/);
    expect(view.hints["app/Models/User.php"]).toMatch(/^\d+% lines covered$/);
    expect(view.nodes[0].methods.total).toBeGreaterThan(0);
  });

  it("runs PHPUnit with PCOV and per-test coverage, merges suites and writes HTML", async () => {
    const cfg: RunConfiguration = { name: "Per test", type: "phpunit", options: { scope: "config", coverageDriver: "pcov", perTestCoverage: "true" } };
    const line = await invoke<CommandLine>("run_command_line", { request: { configuration: cfg, coverage: true } });
    expect(line.args[0]).toBe("-dpcov.enabled=1");
    expect(line.args.slice(-2)).toEqual(["--coverage-xml", "/tmp/forge-test-reports/run-coverage/xml"]);
    expect(line.env?.XDEBUG_MODE).toBe("off");
    const events = await run("coverage-2", cfg, { coverage: true }, 2500);
    const cov = events.find((e) => e.type === "coverage") as Extract<RunEvent, { type: "coverage" }>;
    expect(cov.suite?.perTest).toBe(true);
    const marks = await invoke<LineMark[]>("coverage_file", { path: "app/Models/User.php", id: cov.suite!.id });
    const withTests = marks.find((m) => m.tests?.length);
    expect(withTests?.tests?.[0]).toMatch(/^Tests\\.+::test\w+$/);
    expect(withTests?.hint).toMatch(/\d+ tests?$/);
    // Path coverage needs Xdebug.
    await expect(
      invoke("run_command_line", { request: { configuration: { ...cfg, options: { ...cfg.options, pathCoverage: "true" } }, coverage: true } }),
    ).rejects.toThrow(/Xdebug/);
    // Show Coverage Data with two suites: merged into a new one.
    const suites = await invoke<CoverageSuite[]>("coverage_suites");
    const merged = await invoke<CoverageSuite>("coverage_merge", { ids: [suites[0].id, suites[1].id] });
    expect(merged.name).toBe(`${suites[0].name} + ${suites[1].name}`);
    expect(merged.format).toBe("Merged");
    expect(merged.perTest).toBe(true);
    const html = await invoke<string>("coverage_export_html", { id: merged.id });
    expect(html).toContain("<title>Coverage: Per test + All tests</title>");
    expect(html).toContain("Method, %");
    expect(html).toMatch(/<tr class="covered" title="Hits: \d+/);
  });

  it("reads Istanbul JSON with branches and functions", async () => {
    const { addMockCoverage } = await import("./coverageMock");
    const json = JSON.stringify({
      "/p/src/cart.ts": {
        path: "/p/src/cart.ts",
        statementMap: { "0": { start: { line: 1, column: 0 }, end: { line: 1, column: 9 } }, "1": { start: { line: 2, column: 0 }, end: { line: 2, column: 9 } } },
        fnMap: { "0": { name: "add", decl: { start: { line: 1, column: 0 } }, loc: { start: { line: 1, column: 0 } }, line: 1 } },
        branchMap: { "0": { type: "if", line: 2, locations: [{}, {}] } },
        s: { "0": 2, "1": 2 },
        f: { "0": 2 },
        b: { "0": [2, 0] },
      },
    });
    const suite = await addMockCoverage("vitest", json, { root: "/p", project: "p" });
    expect([suite.format, suite.lineText, suite.branchText, suite.methodText]).toEqual(["Istanbul JSON", "100% (2/2)", "50% (1/2)", "100% (1/1)"]);
    const marks = await invoke<LineMark[]>("coverage_file", { path: "src/cart.ts", id: suite.id });
    expect(marks.map((m) => m.status)).toEqual(["covered", "partial"]);
  });

  it("builds command lines on run targets", async () => {
    const cfg: RunConfiguration = {
      name: "entity",
      type: "busted",
      options: { scope: "file", file: "spec/entity_spec.lua" },
      runOn: { kind: "dockerImage", image: "nickblah/lua:5.4" },
    };
    const line = await invoke<CommandLine>("run_command_line", { request: { configuration: cfg } });
    expect(line.program).toBe("docker");
    expect(line.args.slice(0, 5)).toEqual(["run", "--rm", "-i", "-v", "~/Projects/acme-shop:/opt/project"]);
    expect(line.args.slice(-4)).toEqual(["nickblah/lua:5.4", "busted", "--output=TAP", "spec/entity_spec.lua"]);
    const info = await invoke<{ ideHost: string; mappings: { remote: string }[] }>("run_target_info", { configuration: cfg });
    expect(info.ideHost).toBe("host.docker.internal");
    expect(info.mappings[0].remote).toBe("/opt/project");
  });
});
