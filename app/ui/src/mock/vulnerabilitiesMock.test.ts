import { describe, expect, it } from "vitest";
import { invoke } from "@/ipc";
import type { CheckResult, VulnerabilityProblem } from "@/ipc/vulnerabilities";
import { demoFiles } from "./demoProject";
import { mockFiles } from "./workspaceMock";
import "./vulnerabilitiesMock";

describe("Vulnerabilities mock (forge-vulns through WebAssembly)", () => {
  it("checks the Laravel demo with its recorded composer audit", async () => {
    const check = await invoke<CheckResult>("vulnerabilities_check", { force: true, tools: null });
    expect(check.enabled).toBe(true);
    const composer = check.results.find((r) => r.tool === "composer")!;
    expect(composer.status).toBe("ok");
    expect(composer.annotate).toEqual(["composer.json", "composer.lock"]);
    const framework = composer.vulnerabilities.find((v) => v.package === "laravel/framework")!;
    expect(framework).toMatchObject({ id: "CVE-2025-27515", severity: "moderate", fixed: "12.1.1", installed: "v12.1.0" });
    // package.json without a lock file cannot be audited yet.
    const npm = check.results.find((r) => r.tool === "npm");
    expect(npm?.status).toBe("blocked");

    const only = await invoke<CheckResult>("vulnerabilities_check", { force: false, tools: ["composer"] });
    expect(only.results.map((r) => r.tool)).toEqual(["composer"]);

    const json = demoFiles["composer.json"];
    const problems = await invoke<VulnerabilityProblem[]>("vulnerabilities_problems", {
      path: "composer.json",
      text: json,
      vulnerabilities: composer.vulnerabilities,
      notices: [],
    });
    expect(problems.map((p) => [p.package, p.severity, p.upgrade?.constraint])).toEqual([["laravel/framework", "warning", "^12.1.1"]]);
    const lock = await invoke<VulnerabilityProblem[]>("vulnerabilities_problems", {
      path: "composer.lock",
      text: mockFiles["composer.lock"],
      vulnerabilities: composer.vulnerabilities,
      notices: [],
    });
    expect(lock.map((p) => p.package).sort()).toEqual(["guzzlehttp/psr7", "laravel/framework"]);

    const upgraded = await invoke<string | null>("vulnerabilities_upgrade", { path: "composer.json", text: json, package: "laravel/framework", fixed: "12.1.1" });
    expect(upgraded).toContain('"laravel/framework": "^12.1.1"');
  });
});
