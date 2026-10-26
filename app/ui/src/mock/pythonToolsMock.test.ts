import { describe, expect, it } from "vitest";
import type { TestFile } from "@/ipc/testFrameworks";
import type { ProfileSnapshot, ToxEnv } from "@/ipc/pythonTools";
import type { CommandLine, RunConfiguration } from "@/ipc/run";

// The Python demo is chosen by the page URL, before the mocks load.
history.replaceState(null, "", "/?project=python");
const { invoke } = await import("@/ipc");
const { pythonDemoFiles } = await import("./demoPythonProject");
await import("./runMock");
await import("./pythonMock");
await import("./pythonToolsMock");

describe("PyForge run tools mock (pyforge-testing through WebAssembly)", () => {
  it("discovers pytest tests with parametrized cases and unittest cases", async () => {
    const pricing = await invoke<TestFile>("test_discover", { path: "tests/test_pricing.py", text: pythonDemoFiles["tests/test_pricing.py"] });
    expect(pricing.framework).toBe("pytest");
    const labels = pricing.targets.map((t) => t.label);
    expect(labels).toContain("test_round_cents[2.675-2.68]");
    expect(labels).toContain("TestCart");
    const total = pricing.targets.find((t) => t.label === "test_total_includes_tax")!;
    expect(total.configuration.options).toMatchObject({ scope: "test", test: "TestCart::test_total_includes_tax" });
    // The project uses pytest (pyproject.toml), so unittest cases run with pytest too.
    const settings = await invoke<TestFile>("test_discover", { path: "tests/test_settings.py", text: pythonDemoFiles["tests/test_settings.py"] });
    expect(settings.framework).toBe("pytest");
  });

  it("builds commands, reruns failed tests and reads tox environments", async () => {
    const cfg: RunConfiguration = { name: "pytest in test_pricing.py", type: "pytest", options: { scope: "file", file: "tests/test_pricing.py" } };
    const cmd = await invoke<CommandLine>("run_command_line", { request: { configuration: cfg, runId: "run-1" } });
    expect(cmd.args.slice(0, 3)).toEqual(["-m", "pytest", "tests/test_pricing.py"]);
    expect(cmd.args).toContain("--junitxml");
    const rerun = await invoke<RunConfiguration>("test_rerun_failed", {
      configuration: cfg,
      failed: [{ name: "test_total_includes_tax", path: "tests/test_pricing.py" }],
    });
    expect(rerun.options?.nodes).toBe("tests/test_pricing.py::TestCart::test_total_includes_tax");
    const envs = await invoke<ToxEnv[]>("python_tox_environments", {});
    expect(envs.map((e) => e.name)).toEqual(["py311", "py312", "lint"]);
  });

  it("opens profiles and answers the console", async () => {
    const info = await invoke<{ id: string }>("python_profile_open", { path: "report.pstats" });
    const snap = await invoke<ProfileSnapshot>("python_profile_view", { id: info.id });
    expect(snap.view.hotSpots[0].name).toBe("parse_row");
    expect(snap.view.callTree[0].children?.[0].name).toBe("<module>");
    const started = await invoke<{ id: string }>("python_console_start", { id: "c1" });
    expect(started.id).toBe("c1");
  });
});
