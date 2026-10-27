/**
 * Browser-mode PHP test frameworks: Settings | PHP | Test Frameworks (detected from the
 * demo project's files), gutter discovery, Rerun Failed Tests and Behat step definitions,
 * computed by the Rust `phpforge-test-frameworks` crate (WebAssembly) like in the desktop
 * app; canned Pest (TeamCity), Codeception and Behat (JUnit reports) and PHPSpec (JUnit on
 * stdout) runs of the demo tests; and an in-memory Test History. Nothing runs and nothing
 * is written.
 */
import { registerMock } from "@/ipc";
import type { CommandLine, RunConfiguration, TestNodeData, TestSummary } from "@/ipc/run";
import type { FrameworkConfig, StoredRun, TestRunEntry } from "@/ipc/testFrameworks";
import { callWasm, callWasmSync, forgeWasm } from "@/wasm";
import { mockSettingValue, mockTestFrameworkSpecs } from "./settingsMock";
import type { TestFrameworkSpec } from "@/ipc/testing";
import { mockFiles } from "./workspaceMock";
import type { Step } from "./runMock";
import { jsTestSteps } from "./jsTestsMock";
import { webDemo } from "./demoWebProjects";
import { goContext, goTestSteps } from "./goTestsMock";
import { cargoTestSteps, rustContext } from "./rustTestsMock";
import { minitestSteps, rspecSteps, rubyContext } from "./rubyTestsMock";

export const TEST_TYPES = new Set(["pest", "codeception", "behat", "phpspec"]);

/** The demo's `<root>` as the tools would see it (absolute paths in reports). */
const ROOT = "~/Projects/acme-shop";

/** Configured plus detected frameworks of the demo project. */
async function frameworks(): Promise<FrameworkConfig[]> {
  const wasm = await forgeWasm();
  const files = Object.keys(mockFiles);
  return callWasmSync<FrameworkConfig[]>(wasm.test_frameworks_effective, {
    configured: mockSettingValue("php.testFrameworks") ?? [],
    // `vendor/` is not part of the demo: Pest's script stands for the installed package.
    files: [...files, ...(files.includes("tests/Feature/CartTest.php") ? ["vendor/bin/pest"] : [])],
    composerJson: mockFiles["composer.json"] ?? null,
  });
}

registerMock("test_frameworks_list", () => frameworks());

// Discovery and Rerun Failed Tests of every framework: the PHP ones and the manifests'.
registerMock("test_discover", async ({ path, text }) => {
  const wasm = await forgeWasm();
  const specs = mockTestFrameworkSpecs();
  const texts = mockFiles["package.json"] ? { "package.json": mockFiles["package.json"] } : {};
  const detected = callWasmSync<string[]>(wasm.testing_detect, { specs, files: Object.keys(mockFiles), texts });
  return callWasm(wasm.testing_discover, { path, text, specs, detected });
});

registerMock("test_rerun_failed", async ({ configuration, failed }) =>
  callWasm((await forgeWasm()).testing_rerun_failed, { configuration, failed, specs: mockTestFrameworkSpecs() }),
);

registerMock("test_frameworks_detect", async () => {
  const wasm = await forgeWasm();
  const files = Object.keys(mockFiles);
  return callWasmSync<string[]>(wasm.testing_detect, { specs: mockTestFrameworkSpecs(), files, texts: {} });
});

// ---- Frameworks declared by manifests (busted) -----------------------------------------

/** The manifest framework of a configuration type. */
export const specFor = (type: string): TestFrameworkSpec | undefined => mockTestFrameworkSpecs().find((f) => f.id === type);

interface FrameworkCommand {
  program: string;
  args: string[];
  env?: Record<string, string>;
  report: string;
  flavor: string;
  locationRules?: { pattern: string; replacement: string }[];
  defaultFile?: string;
  coverage?: { file: string; format: string };
}

/** The command of a manifest framework's configuration, built by `forge-testing`. */
export async function frameworkCommand(cfg: RunConfiguration, coverage = false): Promise<FrameworkCommand> {
  const wasm = await forgeWasm();
  return callWasmSync<FrameworkCommand>(wasm.testing_command, {
    configuration: cfg,
    spec: specFor(cfg.type),
    reportDir: "/tmp/forge-test-reports/run",
    coverageFile: coverage ? "/tmp/forge-test-reports/run-coverage/coverage" : null,
    goContext: cfg.type === "go-test" ? goContext() : null,
    rustContext: cfg.type === "cargo-test" ? rustContext() : null,
    rubyContext: cfg.type === "rspec" || cfg.type === "minitest" ? rubyContext() : null,
  });
}

interface BustedTest {
  file: string;
  name: string;
  ms: number;
  fail?: { line: number; message: string; passed: string; expected: string };
  pending?: boolean;
}

const BUSTED: BustedTest[] = [
  { file: "spec/entity_spec.lua", name: "Entity starts with 10 hit points", ms: 1 },
  { file: "spec/entity_spec.lua", name: "Entity damage never goes below zero", ms: 1 },
  { file: "spec/entity_spec.lua", name: "Entity damage kills at zero hit points", ms: 2 },
  { file: "spec/player_spec.lua", name: "Player picks up gold", ms: 1 },
  {
    file: "spec/player_spec.lua",
    name: "Player moves by speed and frame time",
    ms: 3,
    fail: { line: 11, message: "spec/player_spec.lua:14: Expected objects to be equal.", passed: "(number) 4", expected: "(number) 5" },
  },
  { file: "spec/player_spec.lua", name: "Player drops gold on death", ms: 0, pending: true },
];

/** Canned `busted --output=TAP` output for the tests a configuration selects. */
function bustedSteps(cfg: RunConfiguration): Step[] {
  const o = cfg.options ?? {};
  const scope = o.scope || "all";
  let tests = BUSTED;
  if (scope === "file") tests = tests.filter((t) => t.file === o.file);
  if (scope === "directory") tests = tests.filter((t) => t.file.startsWith(`${(o.directory ?? "").replace(/\/$/, "")}/`));
  if (scope === "test") tests = tests.filter((t) => t.file === o.file && t.name.startsWith(o.test ?? ""));
  if (scope === "pattern") {
    const names = (o.pattern ?? "").split("\n").map((p) => p.replace(/%(.)/g, "$1"));
    tests = tests.filter((t) => names.some((n) => t.name.includes(n)));
  }
  const steps: Step[] = [[120, ""]];
  tests.forEach((t, i) => {
    if (t.pending) steps.push([20, `ok ${i + 1} - # SKIP ${t.name}\n`]);
    else if (t.fail)
      steps.push([
        t.ms * 40,
        `not ok ${i + 1} - ${t.name}\n# ${t.file} @ ${t.fail.line}\n# Failure message: ${t.fail.message}\n# Passed in:\n# ${t.fail.passed}\n# Expected:\n# ${t.fail.expected}\n`,
      ]);
    else steps.push([t.ms * 40, `ok ${i + 1} - ${t.name}\n`]);
  });
  steps.push([20, `1..${tests.length}\n`], [10, { code: tests.some((t) => t.fail) ? 1 : 0 }]);
  return steps;
}

/** Canned output of a manifest framework's run. */
export function frameworkSteps(cfg: RunConfiguration): Step[] {
  if (cfg.type === "busted") return bustedSteps(cfg);
  if (cfg.type === "go-test") return goTestSteps(cfg);
  if (cfg.type === "cargo-test") return cargoTestSteps(cfg);
  if (cfg.type === "rspec") return rspecSteps(cfg);
  if (cfg.type === "minitest") return minitestSteps(cfg);
  const js = jsTestSteps(cfg, `~/Projects/${webDemo()?.name ?? "acme-shop"}`);
  if (js) return js;
  return [[100, "Done.\n"], [20, { code: 0 }]];
}

registerMock("behat_step_definitions", async ({ text, line }) => {
  const files = Object.entries(mockFiles)
    .filter(([p]) => p.endsWith(".php"))
    .map(([path, text]) => ({ path, text }));
  return callWasm((await forgeWasm()).behat_step_definitions, { text, line, files });
});

// ---- Command lines ----------------------------------------------------------------------

export interface TestCommand {
  args: string[];
  report: "teamCity" | "jUnitStdout" | "jUnitFile";
  flavor: "generic" | "behat" | "phpSpec";
}

/** The command of a Pest, Codeception, Behat or PHPSpec configuration (after `php`). */
export async function testCommand(cfg: RunConfiguration): Promise<TestCommand> {
  const wasm = await forgeWasm();
  const list = await frameworks();
  const framework = list.find((f) => f.framework === cfg.type && !f.interpreter) ?? list.find((f) => f.framework === cfg.type) ?? null;
  return callWasmSync<TestCommand>(wasm.test_command, {
    configuration: cfg,
    framework,
    reportDir: "/tmp/forge-test-reports/run",
  });
}

export async function testCommandLine(cfg: RunConfiguration): Promise<CommandLine & { report: TestCommand }> {
  const split = (s: string | undefined) => (s ?? "").split(/\s+/).filter(Boolean);
  const cmd = await testCommand(cfg);
  return {
    program: cfg.interpreter || "/usr/bin/php",
    args: [...split(cfg.interpreterOptions), ...cmd.args, ...split(cfg.arguments)],
    cwd: cfg.workingDirectory || "",
    report: cmd,
  };
}

// ---- Canned runs ------------------------------------------------------------------------

const esc = (s: string) => s.replace(/\|/g, "||").replace(/'/g, "|'").replace(/\n/g, "|n").replace(/\[/g, "|[").replace(/]/g, "|]");
const tc = (name: string, attrs: Record<string, string | number>) =>
  `##teamcity[${name} ${Object.entries(attrs)
    .map(([k, v]) => `${k}='${esc(String(v))}'`)
    .join(" ")} flowId='4242']\n`;
const xml = (s: string) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");

/** The tests a configuration selects, by name; `--filter 'a|b'` (Rerun Failed Tests) narrows them. */
function selected<T extends { name: string }>(cfg: RunConfiguration, all: T[], keyOf: (t: T) => string = (t) => t.name): T[] {
  const o = cfg.options ?? {};
  let out = all;
  if (o.scope === "test" && o.test) out = out.filter((t) => keyOf(t).startsWith(o.test!) || keyOf(t).includes(o.test!));
  const filter = /--filter '([^']*)'/.exec(o.testRunnerOptions ?? "");
  if (filter) {
    const names = filter[1].split("|").map((n) => n.replace(/\\(.)/g, "$1"));
    out = out.filter((t) => names.some((n) => keyOf(t) === n || keyOf(t).startsWith(`${n} with`)));
  }
  return out;
}

interface PestTest {
  name: string;
  ms: number;
  fail?: { message: string; expected: string; actual: string; line: number };
}

const PEST_FILE = "tests/Feature/CartTest.php";
const PEST_TESTS: PestTest[] = [
  { name: "an empty cart costs nothing", ms: 3 },
  { name: '`totals` → it adds the tax with data set "ten euros"', ms: 4 },
  { name: '`totals` → it adds the tax with data set "a hundred euros"', ms: 2 },
  {
    name: "`totals` → it rounds to cents",
    ms: 5,
    fail: { message: "Failed asserting that 1.1781 is identical to 1.18.", expected: "1.18", actual: "1.1781", line: 23 },
  },
  { name: "it belongs to a user", ms: 21 },
];

function pestSteps(cfg: RunConfiguration): Step[] {
  const tests = selected(cfg, PEST_TESTS);
  const abs = `${ROOT}/${PEST_FILE}`;
  const steps: Step[] = [
    [80, "\n"],
    [20, tc("testCount", { count: tests.length })],
    [10, tc("testSuiteStarted", { name: "Tests\\Feature\\CartTest", locationHint: `pest_qn://${PEST_FILE}` })],
  ];
  for (const t of tests) {
    steps.push([15, tc("testStarted", { name: t.name, locationHint: `pest_qn://${PEST_FILE}::${t.name}` })]);
    if (t.fail)
      steps.push([
        t.ms * 15,
        tc("testFailed", {
          name: t.name,
          message: t.fail.message,
          details: `at ${abs}:${t.fail.line}\n`,
          type: "comparisonFailure",
          expected: t.fail.expected,
          actual: t.fail.actual,
        }),
      ]);
    steps.push([t.ms * 15, tc("testFinished", { name: t.name, duration: t.ms })]);
  }
  steps.push([5, tc("testSuiteFinished", { name: "Tests\\Feature\\CartTest" })]);
  const failed = tests.filter((t) => t.fail).length;
  steps.push(
    [30, `\n  Tests:    ${failed ? `\x1b[31;1m${failed} failed\x1b[0m, ` : ""}\x1b[32;1m${tests.length - failed} passed\x1b[0m (${tests.length + 2} assertions)\n  Duration: 0.21s\n\n`],
    [10, { code: failed ? 1 : 0 }],
  );
  return steps;
}

interface Case {
  name: string;
  ms: number;
  failure?: string;
  line?: number;
}

const CEST = "tests/Acceptance/LoginCest.php";
const CEST_CASES: Case[] = [
  { name: "loginWithValidCredentials", ms: 412, line: 14 },
  {
    name: "loginWithWrongPassword",
    ms: 288,
    line: 22,
    failure: `Tests\\Acceptance\\LoginCest: Login with wrong password
 Failed asserting that  on page /login
--> Log in
--> contains "These credentials do not match our records.".

${ROOT}/${CEST}:26`,
  },
];

function codeceptionSteps(cfg: RunConfiguration): Step[] {
  const o = cfg.options ?? {};
  let cases = CEST_CASES;
  if (o.scope === "method") cases = cases.filter((c) => c.name === o.method);
  if (/-g failed/.test(o.testRunnerOptions ?? "")) cases = cases.filter((c) => c.failure);
  const failed = cases.filter((c) => c.failure);
  const report = `<?xml version="1.0" encoding="UTF-8"?>
<testsuites>
  <testsuite name="Acceptance" tests="${cases.length}" assertions="${cases.length * 2}" errors="0" failures="${failed.length}" skipped="0" time="0.700">
${cases
  .map(
    (c) =>
      `    <testcase name="${c.name}" class="Tests\\Acceptance\\LoginCest" file="${ROOT}/${CEST}" line="${c.line}" time="${c.ms / 1000}">${c.failure ? `<failure type="PHPUnit\\Framework\\AssertionFailedError">${xml(c.failure)}</failure>` : ""}</testcase>`,
  )
  .join("\n")}
  </testsuite>
</testsuites>`;
  return [
    [120, "Codeception PHP Testing Framework v5.1.2 https://stand-with-ukraine.pp.ua\n\n"],
    [60, `\x1b[1mTests.Acceptance Tests (${cases.length})\x1b[22m ---------------------------------------\n`],
    ...cases.map((c): Step => [c.ms, `${c.failure ? "\x1b[31m✖\x1b[39m" : "\x1b[32m✔\x1b[39m"} LoginCest: ${c.name.replace(/([A-Z])/g, " $1").toLowerCase()} \x1b[90m(${(c.ms / 1000).toFixed(2)}s)\x1b[39m\n`]),
    [40, `---------------------------------------------------------------------\nTime: 00:00.700, Memory: 18.00 MB\n\n${failed.length ? `\x1b[37;41mFAILURES!\x1b[0m\n\x1b[37;41mTests: ${cases.length}, Assertions: ${cases.length * 2}, Failures: ${failed.length}.\x1b[0m\n` : `\x1b[30;42mOK (${cases.length} tests, ${cases.length * 2} assertions)\x1b[0m\n`}`],
    [10, { report }],
    [5, { code: failed.length ? 1 : 0 }],
  ];
}

const FEATURE = "features/checkout.feature";
const SCENARIOS: Case[] = [
  {
    name: "Buying a single product",
    ms: 4,
    line: 10,
    failure: "And the overall basket price should be £9: Failed asserting that 5.95 is identical to 9.0.",
  },
  { name: "Buying several products #1", ms: 2, line: 21 },
  { name: "Buying several products #2", ms: 2, line: 22 },
];

function behatSteps(cfg: RunConfiguration): Step[] {
  const o = cfg.options ?? {};
  let cases = SCENARIOS;
  if (o.scope === "scenario") {
    const line = Number(o.line);
    cases = line === 15 ? cases.filter((c) => c.name.startsWith("Buying several")) : cases.filter((c) => c.line === line);
  }
  if (/--rerun/.test(o.testRunnerOptions ?? "")) cases = cases.filter((c) => c.failure);
  const failed = cases.filter((c) => c.failure);
  const report = `<?xml version="1.0" encoding="UTF-8"?>
<testsuites name="default">
  <testsuite name="Checkout" file="${FEATURE}" tests="${cases.length}" skipped="0" failures="${failed.length}" errors="0" time="0.008">
${cases
  .map(
    (c) =>
      `    <testcase name="${xml(c.name)}" classname="Checkout" status="${c.failure ? "failed" : "passed"}" time="${c.ms / 1000}">${c.failure ? `<failure message="${xml(c.failure)}"></failure>` : ""}</testcase>`,
  )
  .join("\n")}
  </testsuite>
</testsuites>`;
  const dots = cases.map((c) => (c.failure ? "...\x1b[31mF\x1b[39m" : "\x1b[32m...\x1b[39m")).join("");
  return [
    [150, `${dots}\n\n`],
    ...(failed.length
      ? ([[30, `--- \x1b[31mFailed steps:\x1b[39m\n\n001 Scenario: Buying a single product # ${FEATURE}:10\n      And the overall basket price should be £9 # ${FEATURE}:13\n        \x1b[31mFailed asserting that 5.95 is identical to 9.0.\x1b[39m\n\n`]] as Step[])
      : []),
    [20, `${cases.length} scenarios (${failed.length ? `\x1b[32m${cases.length - failed.length} passed\x1b[39m, \x1b[31m${failed.length} failed\x1b[39m` : `\x1b[32m${cases.length} passed\x1b[39m`})\n0m0.03s (12.41Mb)\n`],
    [10, { report }],
    [5, { code: failed.length ? 1 : 0 }],
  ];
}

const EXAMPLES: Case[] = [
  { name: "it is initializable", ms: 1 },
  { name: "it adds the tax to the total", ms: 2 },
  { name: "it rejects negative amounts", ms: 3, failure: 'expected to get exception / throwable of class "InvalidArgumentException", but got none.' },
];

function phpspecSteps(cfg: RunConfiguration): Step[] {
  const o = cfg.options ?? {};
  let cases = EXAMPLES;
  if (o.scope === "method" && o.method) cases = cases.filter((c) => c.name === o.method.replace(/_/g, " "));
  const failed = cases.filter((c) => c.failure);
  const report = `<?xml version="1.0" encoding="UTF-8" ?>
<testsuites time="0.006" tests="${cases.length}" failures="${failed.length}" errors="0">
  <testsuite name="App\\Services\\Checkout" time="0.006" tests="${cases.length}" failures="${failed.length}" errors="0" skipped="0">
${cases
  .map(
    (c) =>
      `    <testcase name="${c.name}" time="${c.ms / 1000}" classname="spec\\App\\Services\\CheckoutSpec" status="${c.failure ? "failed" : "passed"}">${c.failure ? `<failure type="PhpSpec\\Exception\\Example\\FailureException" message="${xml(c.failure)}" />` : ""}</testcase>`,
  )
  .join("\n")}
  </testsuite>
</testsuites>
`;
  return [
    [200, report],
    [10, { code: failed.length ? 1 : 0 }],
  ];
}

/** Canned output of a test framework run. */
export function testSteps(cfg: RunConfiguration): Step[] {
  switch (cfg.type) {
    case "pest":
      return pestSteps(cfg);
    case "codeception":
      return codeceptionSteps(cfg);
    case "behat":
      return behatSteps(cfg);
    default:
      return phpspecSteps(cfg);
  }
}

// ---- Test History -----------------------------------------------------------------------

interface Recorded {
  entry: TestRunEntry;
  configuration: RunConfiguration;
  tree: unknown;
}
const history: Recorded[] = [];
let seq = 0;

/** Remember a finished test run (the desktop backend stores it in Forge's data dir). */
export function recordTestRun(configuration: RunConfiguration, at: number, exported: { tree: unknown; summary: TestSummary; counts: TestSummary["counts"] }): void {
  if (!exported.counts.total) return;
  history.unshift({
    entry: { id: `${at}-${++seq}`, name: configuration.name, type: configuration.type, at, summary: exported.summary.text, counts: exported.counts },
    configuration,
    tree: exported.tree,
  });
  history.splice(10);
}

registerMock("test_history_list", () => history.map((h) => h.entry));
registerMock("test_history_open", async ({ id }): Promise<StoredRun> => {
  const h = history.find((x) => x.entry.id === id);
  if (!h) throw new Error(`no test run ${String(id)}`);
  const { projectRoot } = await import("./runMock");
  const out = await callWasm<{ nodes: TestNodeData[]; summary: TestSummary }>((await forgeWasm()).test_history_import, { tree: h.tree, root: projectRoot() });
  return { entry: h.entry, configuration: h.configuration, ...out };
});
