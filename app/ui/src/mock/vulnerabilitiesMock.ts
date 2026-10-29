/**
 * Browser-mode vulnerable dependency check. Browser mode runs no tools and has no network:
 * the demo projects come with reports recorded from the real tools (`composer audit` for
 * the Laravel demo, `pnpm audit` for the React storefront), and everything else (which
 * audits a project needs, reading the reports, the manifest problems, the Upgrade edit) is
 * computed by the Rust `forge-vulns` crate through WebAssembly, like in the desktop app.
 */
import { registerMock } from "@/ipc";
import type { CommandLine } from "@/ipc/run";
import type { CheckResult, TargetResult, ToolId } from "@/ipc/vulnerabilities";
import { callWasm, forgeWasm } from "@/wasm";
import { mockSettingValue } from "./settingsMock";
import { mockSafeMode } from "./trustMock";
import { mockFiles } from "./workspaceMock";

const delay = (ms: number) => new Promise((r) => setTimeout(r, ms));

/** `composer audit --format=json --locked` of the Laravel demo's composer.lock. */
export const DEMO_COMPOSER_AUDIT = {
  advisories: {
    "guzzlehttp/psr7": [
      {
        advisoryId: "PKSA-8qx3-n5y5-vvnd",
        packageName: "guzzlehttp/psr7",
        affectedVersions: ">=2,<2.4.5|<1.9.1",
        title: "Improper header validation",
        cve: "CVE-2023-29197",
        link: "https://github.com/guzzle/psr7/security/advisories/GHSA-wxmh-65f7-jcvw",
        reportedAt: "2023-04-17T16:00:00+00:00",
        sources: [{ name: "GitHub", remoteId: "GHSA-wxmh-65f7-jcvw" }],
        severity: "medium",
      },
    ],
    "laravel/framework": [
      {
        advisoryId: "PKSA-yhcn-xrg3-68b1",
        packageName: "laravel/framework",
        affectedVersions: ">=12.0.0,<12.1.1",
        title: "File validation bypass with uppercase extensions",
        cve: "CVE-2025-27515",
        link: "https://github.com/laravel/framework/security/advisories/GHSA-78fx-h6xr-vch4",
        reportedAt: "2025-03-05T18:00:00+00:00",
        sources: [{ name: "GitHub", remoteId: "GHSA-78fx-h6xr-vch4" }],
        severity: "medium",
      },
    ],
  },
  abandoned: {},
};

/** `pnpm audit --json` of the React storefront (npm 6 format). */
export const DEMO_PNPM_AUDIT = {
  actions: [],
  advisories: {
    "1107468": {
      findings: [{ version: "7.5.3", paths: [".>react-router-dom>react-router"] }],
      id: 1107468,
      title: "React Router has an XSS vulnerability in framework mode meta exports",
      module_name: "react-router",
      cves: ["CVE-2025-59057"],
      severity: "high",
      vulnerable_versions: ">=7.0.0 <7.9.0",
      patched_versions: ">=7.9.0",
      github_advisory_id: "GHSA-9jcx-v3wj-wh4m",
      url: "https://github.com/advisories/GHSA-9jcx-v3wj-wh4m",
    },
    "1107633": {
      findings: [{ version: "6.3.5", paths: [".>vite", ".>vitest>vite"] }],
      id: 1107633,
      title: "Vite middleware may serve files starting with the same name with the public directory",
      module_name: "vite",
      cves: ["CVE-2025-58751"],
      severity: "low",
      vulnerable_versions: ">=6.0.0 <=6.3.5",
      patched_versions: ">=6.3.6",
      github_advisory_id: "GHSA-g4jq-h2w9-997c",
      url: "https://github.com/advisories/GHSA-g4jq-h2w9-997c",
    },
    "1107634": {
      findings: [{ version: "6.3.5", paths: [".>vite"] }],
      id: 1107634,
      title: "Vite's server.fs settings were not applied to HTML files",
      module_name: "vite",
      cves: ["CVE-2025-58752"],
      severity: "low",
      vulnerable_versions: ">=6.0.0 <=6.3.5",
      patched_versions: ">=6.3.6",
      github_advisory_id: "GHSA-jqfw-vq24-v9c3",
      url: "https://github.com/advisories/GHSA-jqfw-vq24-v9c3",
    },
  },
  muted: [],
  metadata: { vulnerabilities: { info: 0, low: 2, moderate: 0, high: 1, critical: 0 }, dependencies: 412 },
};

/** Recorded tool output per tool; tools without a recording report nothing found. */
const RECORDED: Partial<Record<ToolId, { stdout: string; stderr?: string }>> = {
  composer: { stdout: JSON.stringify(DEMO_COMPOSER_AUDIT, null, 4) },
  pnpm: { stdout: JSON.stringify(DEMO_PNPM_AUDIT, null, 2) },
  // The demo interpreter has no pip-audit: shows the install offer.
  "pip-audit": { stdout: "", stderr: "/home/dev/shop-api/.venv/bin/python: No module named pip_audit" },
};

const EMPTY: Partial<Record<ToolId, string>> = {
  npm: '{"auditReportVersion": 2, "vulnerabilities": {}}',
  "yarn-classic": '{"type":"auditSummary","data":{"vulnerabilities":{}}}',
  "yarn-berry": "{}",
  "cargo-audit": '{"vulnerabilities": {"found": false, "count": 0, "list": []}, "warnings": {}}',
  govulncheck: '{"config": {"protocol_version": "v1.0.0"}}',
};

registerMock("vulnerabilities_check", async ({ tools }): Promise<CheckResult> => {
  if (mockSettingValue("vulnerabilities.enabled") === false) return { enabled: false, results: [] };
  if (mockSafeMode()) throw new Error("Checking dependencies for vulnerabilities is not allowed in Safe Mode. Trust the project to allow it.");
  await delay(500);
  const w = await forgeWasm();
  const files = Object.keys(mockFiles)
    .filter((p) => !p.includes("/") || p.endsWith("composer.json") || p.endsWith("composer.lock"))
    .map((path) => ({ path, text: mockFiles[path] }));
  const targets = await callWasm<TargetResult[]>(w.vulnerabilities_detect, {
    files,
    options: {
      composerJson: String(mockSettingValue("php.composer.json") || "composer.json"),
      dependencyCheckReport: String(mockSettingValue("vulnerabilities.dependencyCheckReport") || ""),
    },
  });
  const only = tools as ToolId[] | null | undefined;
  const checkedAt = Math.floor(Date.now() / 1000);
  const results: TargetResult[] = [];
  for (const target of targets.filter((t) => !only || only.includes(t.tool))) {
    if (target.blocked) {
      results.push({ ...target, status: "blocked", message: target.blocked, cached: false, vulnerabilities: [] });
      continue;
    }
    const recorded = RECORDED[target.tool] ?? { stdout: EMPTY[target.tool] ?? "{}" };
    results.push(
      await callWasm<TargetResult>(w.vulnerabilities_result, {
        target,
        ...recorded,
        lock: target.lockfile ? mockFiles[target.lockfile] : undefined,
        checkedAt,
      }),
    );
  }
  return { enabled: true, results };
});

registerMock("vulnerabilities_problems", async ({ path, text, vulnerabilities, notices }) => {
  const w = await forgeWasm();
  return callWasm(w.vulnerabilities_problems, { path, text, vulnerabilities, notices });
});

registerMock("vulnerabilities_upgrade", async ({ path, text, package: pkg, fixed }) => {
  const w = await forgeWasm();
  return callWasm(w.vulnerabilities_upgrade, { path, text, package: pkg, fixed });
});

registerMock("vulnerabilities_install_command", ({ tool, dir, package: pkg, fixed }): CommandLine => {
  const cwd = `/home/dev/demo${dir ? `/${dir}` : ""}`;
  switch (tool as ToolId) {
    case "composer":
      return { program: "composer", args: ["update", String(pkg), "--with-dependencies", "--no-interaction"], cwd, env: {} };
    case "pip-audit":
      return { program: ".venv/bin/python", args: ["-m", "pip", "install", pkg ? `${pkg}>=${fixed}` : "pip-audit"], cwd, env: {} };
    case "cargo-audit":
      return { program: "cargo", args: ["update", "-p", String(pkg)], cwd, env: {} };
    case "govulncheck":
      return { program: "go", args: ["mod", "tidy"], cwd, env: {} };
    case "pnpm":
      return { program: "pnpm", args: ["install"], cwd, env: {} };
    case "yarn-classic":
    case "yarn-berry":
      return { program: "yarn", args: ["install"], cwd, env: {} };
    default:
      return { program: "npm", args: ["install"], cwd, env: {} };
  }
});
