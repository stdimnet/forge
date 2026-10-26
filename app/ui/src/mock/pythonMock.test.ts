import { describe, expect, it } from "vitest";
import type { CurrentInterpreter, PackageListing, PythonProblem } from "@/ipc/python";

// The Python demo is chosen by the page URL, before the mocks load.
history.replaceState(null, "", "/?project=python");
const { invoke } = await import("@/ipc");
const { pythonDemoFiles } = await import("./demoPythonProject");
await import("./pythonMock");

describe("PyForge mock (Rust inspections through WebAssembly)", () => {
  it("reports unsatisfied requirements and undeclared imports", async () => {
    const reqs = await invoke<PythonProblem[]>("python_inspect", {
      path: "requirements-dev.txt",
      content: pythonDemoFiles["requirements-dev.txt"],
    });
    expect(reqs.map((p) => [p.line, p.message])).toEqual([
      [3, "Package requirement 'ruff==0.7.1' is not satisfied"],
      [4, "Package requirement 'mypy>=1.11' is not satisfied"],
      [5, "Package requirement 'httpx>=0.28' is not satisfied (installed: 0.27.2)"],
    ]);
    const main = await invoke<PythonProblem[]>("python_inspect", {
      path: "src/shop_api/main.py",
      content: pythonDemoFiles["src/shop_api/main.py"],
    });
    expect(main.map((p) => p.message)).toEqual([
      "Package containing module 'yaml' is not listed in the project requirements",
      "Package containing module 'rich' is not listed in the project requirements",
    ]);
    expect(main[0].fixes[0]).toMatchObject({ kind: "addRequirement", requirement: "PyYAML", target: "pyproject.toml" });
  });

  it("adds requirements and completes versions", async () => {
    const edit = await invoke<{ path: string; text: string }>("python_add_requirement", { requirement: "PyYAML", target: "pyproject.toml", current: null });
    expect(edit.text).toContain('    "uvicorn>=0.30",\n    "PyYAML",\n]');
    const content = "fastapi==0.1";
    const done = await invoke<{ from: number; items: { label: string }[] }>("python_complete", { path: "requirements.txt", content, offset: content.length });
    expect(done.from).toBe(9);
    expect(done.items.map((i) => i.label)).toEqual(["0.115.4", "0.115.0"]);
  });

  it("installs packages with the environment's manager", async () => {
    const cur = await invoke<CurrentInterpreter>("python_interpreter_current");
    expect(cur.label).toBe("Python 3.12 (shop-api)");
    const cmd = await invoke<{ program: string; args: string[] }>("python_packages_command", { action: "install", packages: ["rich"] });
    expect([cmd.program, ...cmd.args].join(" ")).toBe("uv pip install --python ~/Projects/shop-api/.venv/bin/python rich");
    const list = await invoke<PackageListing>("python_packages_list", {});
    expect(list.manager).toBe("uv");
    expect(list.packages.find((p) => p.name === "rich")?.version).toBe("13.9.3");
  });
});
