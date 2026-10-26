/**
 * The Go demo's context and canned `go test -json` runs, without side effects (imported by
 * the test framework mock and goMock.ts).
 */
import type { GoSdk, ModuleRoot } from "@/ipc/go";
import type { RunConfiguration } from "@/ipc/run";
import { GO_MODULE } from "./demoGoProject";
import type { Step } from "./runMock";
import { mockSettingValue } from "./settingsMock";

export const SDKS: GoSdk[] = [
  { root: "/usr/local/go", version: "1.24.7", source: "PATH" },
  { root: "/home/anna/sdk/go1.23.12", version: "1.23.12", source: "~/sdk" },
  { root: "/usr/lib/go", version: "1.22.2", source: "/usr/lib/go" },
];

export const MODULES: ModuleRoot[] = [{ path: GO_MODULE, dir: "" }];

/** The project's Go context as `commands::go::context` builds it. */
export function goContext() {
  const goroot = String(mockSettingValue("go.goroot") ?? "") || SDKS[0].root;
  return {
    goroot,
    gopath: String(mockSettingValue("go.gopath") ?? ""),
    goflags: String(mockSettingValue("go.goflags") ?? ""),
    buildTags: (mockSettingValue("go.buildTags") as string[] | undefined) ?? [],
    env: {},
    modules: MODULES,
    windows: false,
  };
}

// ---- go test -json ------------------------------------------------------------------------------

interface GoCase {
  pkg: string;
  dir: string;
  name: string;
  /** Output lines of a failure. */
  fail?: string;
  ms: number;
}

const CASES: GoCase[] = [
  { pkg: "internal/money", dir: "internal/money", name: "TestRound", ms: 0 },
  { pkg: "internal/money", dir: "internal/money", name: "TestRound/half_up", ms: 0 },
  { pkg: "internal/money", dir: "internal/money", name: "TestRound/negative", ms: 0, fail: "    money_test.go:16: got -1.00, want -2.00\n" },
  { pkg: "internal/money", dir: "internal/money", name: "TestRound/zero", ms: 0 },
  { pkg: "internal/money", dir: "internal/money", name: "TestParse", ms: 0 },
  { pkg: "internal/money", dir: "internal/money", name: "TestSum", ms: 1 },
  { pkg: "internal/money", dir: "internal/money", name: "FuzzParse", ms: 0 },
  { pkg: "internal/money", dir: "internal/money", name: "FuzzParse/seed#0", ms: 0 },
  { pkg: "internal/money", dir: "internal/money", name: "FuzzParse/seed#1", ms: 0 },
  { pkg: "internal/money", dir: "internal/money", name: "ExampleAmount_String", ms: 0 },
  { pkg: "internal/report", dir: "internal/report", name: "TestRender", ms: 2 },
  { pkg: "internal/report", dir: "internal/report", name: "TestTotal", ms: 0 },
];

function selectCases(cfg: RunConfiguration): GoCase[] {
  const o = cfg.options ?? {};
  const scope = o.scope || "all";
  const dirOf = (p: string) => p.replace(/\/[^/]*$/, "");
  const dir = (o.directory || (o.file ? dirOf(o.file) : "")).replace(/\/$/, "");
  const top = (n: string) => n.split("/")[0];
  const inDir = (c: GoCase) => (scope === "directory" && o.recursive !== "false" ? c.dir === dir || c.dir.startsWith(`${dir}/`) : c.dir === dir);
  if (scope === "all") return CASES;
  if (scope === "directory") return CASES.filter(inDir);
  if (scope === "file") {
    const tests = (o.tests ?? "").split("|").filter(Boolean);
    return CASES.filter((c) => inDir(c) && (!tests.length || tests.includes(top(c.name))));
  }
  if (scope === "test") {
    const t = o.test ?? "";
    return CASES.filter((c) => inDir(c) && (c.name === t || c.name.startsWith(`${t}/`) || (t.includes("/") && c.name === top(t))));
  }
  const pattern = new RegExp(o.pattern ?? "^$");
  const packages = (o.packages ?? "").split("\n").filter(Boolean);
  return CASES.filter((c) => pattern.test(top(c.name)) && (!packages.length || packages.includes(c.dir)));
}

/** Canned `go test -json` output (test2json lines) for a Go Test configuration. */
export function goTestSteps(cfg: RunConfiguration): Step[] {
  const o = cfg.options ?? {};
  const ev = (action: string, pkg: string, extra: Record<string, unknown> = {}) =>
    `${JSON.stringify({ Time: "2026-10-26T11:22:53.886Z", Action: action, Package: `${GO_MODULE}/${pkg}`, ...extra })}\n`;
  const steps: Step[] = [[150, ""]];
  if (o.kind === "bench") {
    const pkg = o.directory || "internal/report";
    const name = (o.test || "BenchmarkReport").replace(/^\^|\$$/g, "");
    steps.push(
      [300, ev("start", pkg)],
      [10, ev("output", pkg, { Output: "goos: linux\n" })],
      [10, ev("output", pkg, { Output: "goarch: amd64\n" })],
      [10, ev("output", pkg, { Output: `pkg: ${GO_MODULE}/${pkg}\n` })],
      [900, ev("output", pkg, { Output: `${name}-8   \t  612844\t      1897 ns/op\t     624 B/op\t      14 allocs/op\n` })],
      [10, ev("output", pkg, { Output: "PASS\n" })],
      [10, ev("output", pkg, { Output: `ok  \t${GO_MODULE}/${pkg}\t1.402s\n` })],
      [10, ev("pass", pkg, { Elapsed: 1.402 })],
      [10, { code: 0 }],
    );
    return steps;
  }
  const cases = selectCases(cfg);
  const failedTop = new Set(cases.filter((c) => c.fail).map((c) => c.name.split("/")[0]));
  const byPkg = new Map<string, GoCase[]>();
  for (const c of cases) byPkg.set(c.pkg, [...(byPkg.get(c.pkg) ?? []), c]);
  let failed = false;
  for (const [pkg, list] of byPkg) {
    steps.push([200, ev("start", pkg)]);
    // Parents finish after their subtests.
    const pending: GoCase[] = [];
    const finish = (c: GoCase) => {
      const fails = !!c.fail || (!c.name.includes("/") && failedTop.has(c.name));
      if (fails) failed = true;
      steps.push([10, ev("output", pkg, { Test: c.name, Output: `--- ${fails ? "FAIL" : "PASS"}: ${c.name} (0.00s)\n` })]);
      steps.push([10, ev(fails ? "fail" : "pass", pkg, { Test: c.name, Elapsed: c.ms / 1000 })]);
    };
    for (const c of list) {
      while (pending.length && !c.name.startsWith(`${pending[pending.length - 1].name}/`)) finish(pending.pop()!);
      steps.push([40 + c.ms * 20, ev("run", pkg, { Test: c.name })]);
      steps.push([5, ev("output", pkg, { Test: c.name, Output: `=== RUN   ${c.name}\n` })]);
      if (c.fail) steps.push([10, ev("output", pkg, { Test: c.name, Output: c.fail })]);
      if (list.some((x) => x.name.startsWith(`${c.name}/`))) pending.push(c);
      else finish(c);
    }
    while (pending.length) finish(pending.pop()!);
    const pkgFailed = list.some((c) => c.fail);
    steps.push([20, ev("output", pkg, { Output: pkgFailed ? `FAIL\n` : "PASS\n" })]);
    steps.push([10, ev("output", pkg, { Output: `${pkgFailed ? "FAIL" : "ok  "}\t${GO_MODULE}/${pkg}\t0.012s\n` })]);
    steps.push([10, ev(pkgFailed ? "fail" : "pass", pkg, { Elapsed: 0.012 })]);
  }
  steps.push([10, { code: failed ? 1 : 0 }]);
  return steps;
}

