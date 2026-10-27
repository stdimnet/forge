import { describe, expect, it } from "vitest";
import { invoke } from "@/ipc";
import type { CleanupPlan, ProfilesState, Report } from "@/ipc/inspections";
import { callWasm, forgeWasm } from "@/wasm";
import { mockFiles } from "./workspaceMock";
import "./settingsMock";
import "./phpMetaMock";
import "./inspectionsMock";

const LEGACY = "app/Services/LegacyPricing.php";

describe("inspections mock", () => {
  it("inspects the demo project with the profile, duplicates and data flow", async () => {
    const report = await invoke<Report>("inspections_run", {
      request: { runId: 1, scope: { kind: "directory", path: "app/Services", label: "Directory 'app/Services'" } },
    });
    const legacy = report.problems.filter((p) => p.path === LEGACY).map((p) => [p.inspection, p.line + 1]);
    expect(legacy).toEqual([
      ["DuplicatedCode", 12],
      ["DuplicatedCode", 26],
      ["PhpUnusedAssignment", 42],
      ["PhpConstantCondition", 45],
      ["PhpVariableIsAlwaysNull", 50],
      ["PhpUndefinedVariable", 58],
    ]);
    expect(report.duplicates).toHaveLength(1);
    expect(report.problems.find((p) => p.inspection === "PhpUnusedAssignment")?.severityName).toBe("Weak Warning");
  });

  it("honours the profile and suppression comments", async () => {
    const state = await invoke<ProfilesState>("inspections_profiles");
    const off: ProfilesState = structuredClone(state);
    off.profiles[1].tools = { PhpConstantCondition: { enabled: false } };
    await invoke("inspections_save", { profiles: off });
    const original = mockFiles[LEGACY];
    mockFiles[LEGACY] = original.replace("        $customer = null;", "        $customer = null;\n        /** @noinspection PhpVariableIsAlwaysNull */");
    try {
      const report = await invoke<Report>("inspections_run", { request: { runId: 2, scope: { kind: "file", path: LEGACY, label: "File" } } });
      const ids = report.problems.map((p) => p.inspection);
      expect(ids).not.toContain("PhpConstantCondition");
      expect(ids).not.toContain("PhpVariableIsAlwaysNull");
      expect(ids).toContain("PhpUndefinedVariable");
    } finally {
      mockFiles[LEGACY] = original;
      await invoke("inspections_save", { profiles: state });
    }
  });

  it("plans Code Cleanup and applies fixes to the demo files", async () => {
    const plan = await invoke<CleanupPlan>("inspections_cleanup_plan", { request: { runId: 3, scope: { kind: "file", path: LEGACY, label: "File" } } });
    expect(plan.files).toHaveLength(1);
    expect(plan.files[0].after).not.toContain("$prefix = 'ORD';");
    expect(plan.files[0].after).toContain("$prefix = 'INV';");
    const html = await invoke<string>("inspections_export_report", {
      report: await invoke<Report>("inspections_run", { request: { runId: 4, scope: { kind: "file", path: LEGACY, label: "File" } } }),
      format: "html",
    });
    expect(html).toContain("PHP / Data flow");
  });

  it("filters editor diagnostics through the selected profile", async () => {
    const w = await forgeWasm();
    const registry = await invoke<{ registry: unknown }>("inspections_registry");
    const profile = { name: "P", level: "project", tools: { RsClippy: { severity: "ERROR" }, PhpUnusedAssignment: { enabled: false } } };
    await callWasm(w.inspections_configure, { registry: registry.registry, profile });
    const diag = { range: { start: { line: 0, character: 0 }, end: { line: 0, character: 1 } }, severity: "warning", message: "m" };
    const clippy = await callWasm<{ severity: string; inspection: string }[]>(w.inspections_apply, { path: "src/main.rs", source: "clippy", diagnostics: [diag] });
    expect(clippy[0]).toMatchObject({ severity: "error", inspection: "RsClippy" });
    const php = await callWasm<unknown[]>(w.inspections_apply, {
      path: "a.php",
      source: "php-inspections",
      diagnostics: [{ ...diag, code: "PhpUnusedAssignment" }],
    });
    expect(php).toEqual([]);
  });
});
