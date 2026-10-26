/**
 * Browser-mode PyForge run tools for the FastAPI demo (`?project=python`): pytest,
 * Unittests and tox runs (command lines, discovery, Rerun Failed Tests, tox environments
 * and profiles computed by the Rust crate `pyforge-testing` through WebAssembly), canned
 * results rendered by `forge-output`, debug runs under debugpy, Profile with a demo
 * snapshot, and a small simulated Python Console. Nothing touches a real project.
 */
import { emitMock, mockHandler, registerMock } from "@/ipc";
import type { CommandLine, ConsoleLine, RunConfiguration, RunEvent, RunRequest } from "@/ipc/run";
import type { ConsoleVariable, ProfileInfo, ProfileView } from "@/ipc/pythonTools";
import { callWasmSync, forgeWasm } from "@/wasm";
import { isPythonDemo, pythonDemoName } from "./demoPythonProject";
import { registerMockRunHandler, startMockRun, type Step } from "./runMock";
import { mockSettingValue } from "./settingsMock";
import { mockFiles } from "./workspaceMock";

const ROOT = `~/Projects/${pythonDemoName}`;
const PYTHON = `${ROOT}/.venv/bin/python`;
const PY_TYPES = new Set(["pytest", "unittest", "tox"]);

const wasmCall = async <T>(fn: string, input: unknown): Promise<T> => {
  const wasm = await forgeWasm();
  return callWasmSync<T>((wasm as unknown as Record<string, (json: string) => string>)[fn], input);
};

const files = () => Object.fromEntries(Object.entries(mockFiles).filter(([, v]) => typeof v === "string")) as Record<string, string>;
const setting = (key: string, fallback = "") => String(mockSettingValue(key) ?? fallback);

// ---- tests ---------------------------------------------------------------------------------

const previousDiscover = mockHandler("test_discover");
registerMock("test_discover", async (args) => {
  const path = String(args.path);
  if (path.toLowerCase().endsWith(".py"))
    return wasmCall("python_test_discover", { path, text: args.text, runner: setting("python.testing.runner", "auto"), files: files() });
  return previousDiscover?.(args);
});

const previousRerun = mockHandler("test_rerun_failed");
registerMock("test_rerun_failed", async (args) => {
  const cfg = args.configuration as RunConfiguration;
  if (cfg.type === "pytest" || cfg.type === "unittest") {
    const out = await wasmCall<RunConfiguration | null>("python_rerun_failed", { configuration: cfg, failed: args.failed, files: files() });
    return out ? { ...out, env: cfg.env, interpreter: cfg.interpreter, runOn: cfg.runOn } : null;
  }
  return previousRerun?.(args);
});

registerMock("python_testing_detect", () => wasmCall("python_testing_detect", { files: files() }));
registerMock("python_tox_environments", () => wasmCall("python_tox_environments", { files: files() }));

interface PythonCommand {
  program?: string;
  args: string[];
  env?: Record<string, string>;
  report: string;
  flavor: string;
  reportFiles: boolean;
  defaultFile?: string;
  coverageFile?: string;
}

const REPORTS = "/tmp/forge-test-reports";

async function testCommand(cfg: RunConfiguration, runId: string, head: string[], coverage: boolean): Promise<PythonCommand> {
  return wasmCall<PythonCommand>("python_test_command", {
    configuration: cfg,
    cwd: ROOT,
    reportDir: `${REPORTS}/${runId}`,
    coverageFile: coverage ? `${REPORTS}/${runId}-coverage/coverage.xml` : null,
    reporter: setting("python.pytest.reporter", "junitxml"),
    toxProgram: setting("python.tox.path"),
    head,
  });
}

/** The demo's tests: (file, class?, name, line, outcome). */
interface DemoCase {
  file: string;
  cls?: string;
  name: string;
  line: number;
  ms: number;
  fail?: { message: string; expected: string; actual: string; text: string };
  skip?: string;
}

const PRICING_FAIL = `    @pytest.mark.parametrize(
        ("amount", "expected"),
        [
            (1.1781, 1.18),
            (2.675, 2.68),
            pytest.param(0.0, 0.0, id="zero"),
        ],
    )
    def test_round_cents(amount, expected):
>       assert round_cents(amount) == expected
E       assert 2.67 == 2.68
E        +  where 2.67 = round_cents(2.675)

tests/test_pricing.py:17: AssertionError`;

const CASES: DemoCase[] = [
  { file: "tests/test_orders.py", name: "test_health", line: 7, ms: 12 },
  { file: "tests/test_orders.py", name: "test_unknown_order_is_404", line: 11, ms: 9 },
  { file: "tests/test_pricing.py", name: "test_round_cents[1.1781-1.18]", line: 15, ms: 1 },
  {
    file: "tests/test_pricing.py",
    name: "test_round_cents[2.675-2.68]",
    line: 15,
    ms: 1,
    fail: { message: "assert 2.67 == 2.68", expected: "2.68", actual: "2.67", text: PRICING_FAIL },
  },
  { file: "tests/test_pricing.py", name: "test_round_cents[zero]", line: 15, ms: 1 },
  { file: "tests/test_pricing.py", name: "test_discount_rejects_out_of_range", line: 19, ms: 1 },
  { file: "tests/test_pricing.py", cls: "TestCart", name: "test_empty_cart_total", line: 25, ms: 2 },
  { file: "tests/test_pricing.py", cls: "TestCart", name: "test_total_includes_tax", line: 28, ms: 3 },
  { file: "tests/test_pricing.py", cls: "TestCart", name: "test_voucher", line: 33, ms: 0, skip: "vouchers move to the checkout service" },
  { file: "tests/test_settings.py", cls: "SettingsTest", name: "test_defaults", line: 9, ms: 4 },
  { file: "tests/test_settings.py", cls: "SettingsTest", name: "test_port_from_environment", line: 12, ms: 5 },
];

const nodeId = (c: DemoCase) => [c.file, c.cls, c.name].filter(Boolean).join("::");

/** The cases a pytest command selects (its node ids, files or `-k`). */
function selected(args: string[]): DemoCase[] {
  const at = args.indexOf("pytest");
  const rest = args.slice(at + 1);
  const targets: string[] = [];
  let keyword = "";
  for (let i = 0; i < rest.length; i++) {
    if (rest[i] === "-k") keyword = rest[++i] ?? "";
    else if (rest[i] === "--junitxml" || rest[i] === "-o" || rest[i] === "--report-log") i++;
    else if (!rest[i].startsWith("-")) targets.push(rest[i]);
  }
  let cases = CASES;
  if (targets.length)
    cases = cases.filter((c) => targets.some((t) => nodeId(c) === t || nodeId(c).startsWith(`${t}::`) || nodeId(c).startsWith(`${t}[`) || c.file === t || c.file.startsWith(`${t.replace(/\/$/, "")}/`)));
  if (keyword) {
    const words = keyword.split(/\s+and\s+/).map((w) => w.trim());
    cases = cases.filter((c) => words.every((w) => (w.startsWith("not ") ? !nodeId(c).includes(w.slice(4)) : nodeId(c).includes(w))));
  }
  return cases;
}

const xml = (s: string) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");

function junit(cases: DemoCase[]): string {
  const rows = cases.map((c) => {
    const classname = [c.file.replace(/\.py$/, "").replace(/\//g, "."), c.cls].filter(Boolean).join(".");
    const head = `<testcase classname="${classname}" name="${xml(c.name)}" file="${c.file}" line="${c.line}" time="${(c.ms / 1000).toFixed(3)}"`;
    if (c.fail) return `${head}><failure message="${xml(c.fail.message)}">${xml(c.fail.text)}</failure></testcase>`;
    if (c.skip) return `${head}><skipped type="pytest.skip" message="${xml(c.skip)}">${c.file}:${c.line}: ${xml(c.skip)}</skipped></testcase>`;
    return `${head} />`;
  });
  const failures = cases.filter((c) => c.fail).length;
  const skipped = cases.filter((c) => c.skip).length;
  return `<?xml version="1.0" encoding="utf-8"?><testsuites><testsuite name="pytest" errors="0" failures="${failures}" skipped="${skipped}" tests="${cases.length}" time="0.412">${rows.join("")}</testsuite></testsuites>`;
}

const green = (s: string) => `\x1b[32m${s}\x1b[0m`;
const red = (s: string) => `\x1b[31m${s}\x1b[0m`;
const yellow = (s: string) => `\x1b[33m${s}\x1b[0m`;
const bold = (s: string) => `\x1b[1m${s}\x1b[0m`;

function pytestSteps(cases: DemoCase[], coverage: boolean): Step[] {
  const files = [...new Set(cases.map((c) => c.file))];
  const steps: Step[] = [
    [80, `${bold("============================= test session starts ==============================")}\n`],
    [10, `platform linux -- Python 3.12.7, pytest-8.3.3, pluggy-1.5.0\nrootdir: ${ROOT}\nconfigfile: pyproject.toml\n${coverage ? "plugins: cov-5.0.0, anyio-4.6.0\n" : "plugins: anyio-4.6.0\n"}`],
    [40, `collected ${cases.length} item${cases.length === 1 ? "" : "s"}\n\n`],
  ];
  for (const f of files) {
    const marks = cases
      .filter((c) => c.file === f)
      .map((c) => (c.fail ? red("F") : c.skip ? yellow("s") : green(".")))
      .join("");
    steps.push([120, `${f} ${marks}\n`]);
  }
  const failed = cases.filter((c) => c.fail);
  if (failed.length) {
    steps.push([30, `\n${bold("=================================== FAILURES ===================================")}\n`]);
    for (const c of failed) steps.push([10, `${red(bold(`_________________________ ${c.name} _________________________`))}\n\n${c.fail!.text}\n`]);
  }
  if (coverage)
    steps.push([
      40,
      `\n---------- coverage: platform linux, python 3.12.7-final-0 -----------\nName                        Stmts   Miss  Cover\n-----------------------------------------------\nsrc/shop_api/__init__.py        1      0   100%\nsrc/shop_api/pricing.py        18      2    89%\nsrc/shop_api/settings.py        8      0   100%\n-----------------------------------------------\nTOTAL                          27      2    93%\nCoverage XML written to file .forge/coverage.xml\n`,
    ]);
  const passed = cases.filter((c) => !c.fail && !c.skip).length;
  const summary = [failed.length ? red(`${failed.length} failed`) : "", green(`${passed} passed`), cases.some((c) => c.skip) ? yellow(`${cases.filter((c) => c.skip).length} skipped`) : ""]
    .filter(Boolean)
    .join(", ");
  steps.push([20, `${failed.length ? `${bold("=========================== short test summary info ============================")}\nFAILED ${failed.map((c) => `${nodeId(c)} - ${c.fail!.message}`).join("\nFAILED ")}\n` : ""}${failed.length ? red("=") : green("=")}======================= ${summary} in 0.41s ========================\n`]);
  steps.push([10, { report: junit(cases) }]);
  steps.push([10, { code: failed.length ? 1 : 0 }]);
  return steps;
}

const esc = (s: string) => s.replace(/\|/g, "||").replace(/'/g, "|'").replace(/\n/g, "|n").replace(/\[/g, "|[").replace(/]/g, "|]");
const tc = (name: string, attrs: Record<string, string | number>) =>
  `##teamcity[${name} ${Object.entries(attrs)
    .map(([k, v]) => `${k}='${esc(String(v))}'`)
    .join(" ")}]\n`;

function unittestSteps(args: string[]): Step[] {
  const file = args[args.indexOf("--file") + 1];
  const test = args.includes("--test") ? args[args.indexOf("--test") + 1] : "";
  const cases = CASES.filter((c) => c.file === "tests/test_settings.py" && (!file || args.indexOf("--file") < 0 || c.file === file) && (!test || `${c.cls}.${c.name}`.startsWith(test)));
  const steps: Step[] = [[40, tc("testCount", { count: cases.length })], [10, tc("testSuiteStarted", { name: "tests.test_settings", locationHint: "python://tests/test_settings.py" })]];
  steps.push([5, tc("testSuiteStarted", { name: "SettingsTest", locationHint: "python://tests/test_settings.py:8::SettingsTest" })]);
  for (const c of cases) {
    steps.push([60, tc("testStarted", { name: c.name, locationHint: `python://${c.file}:${c.line}::${c.name}` })]);
    steps.push([c.ms, tc("testFinished", { name: c.name, duration: c.ms })]);
  }
  steps.push([5, tc("testSuiteFinished", { name: "SettingsTest" })], [5, tc("testSuiteFinished", { name: "tests.test_settings" })]);
  steps.push([10, `\nRan ${cases.length} tests in 0.011s\n\nOK\n`, "stderr"], [10, { code: 0 }]);
  return steps;
}

function toxSteps(args: string[]): Step[] {
  const e = args.indexOf("-e");
  const envs = e >= 0 ? args[e + 1].split(",") : ["py311", "py312", "lint"];
  const steps: Step[] = [];
  for (const env of envs) {
    if (env === "lint") {
      steps.push(
        [200, "lint: install_deps> python -I -m pip install ruff\n"],
        [300, "lint: commands[0]> ruff check src tests\n"],
        [150, "src/shop_api/main.py:3:8: F401 [*] `yaml` imported but unused\nFound 1 error.\n[*] 1 fixable with the `--fix` option.\n"],
        [20, `lint: exit 1 (0.05 seconds) ${ROOT}> ruff check src tests pid=48233\n`],
        [10, "lint: FAIL ✖ in 1.2 seconds\n"],
      );
    } else {
      steps.push(
        [200, `${env}: install_deps> python -I -m pip install 'pytest>=8'\n`],
        [250, `${env}: install_package> python -I -m pip install --force-reinstall --no-deps ${ROOT}/.tox/.tmp/package/1/shop_api-0.4.0.tar.gz\n`],
        [250, `${env}: commands[0]> pytest\n`],
        [300, `........s..                                                              [100%]\n10 passed, 1 skipped in 0.38s\n`],
        [10, `${env}: OK ✔ in 3.1 seconds\n`],
      );
    }
  }
  const summary = envs.map((env) => (env === "lint" ? "  lint: FAIL code 1 (1.20=setup[1.15]+cmd[0.05] seconds)" : `  ${env}: OK (3.10=setup[2.20]+cmd[0.90] seconds)`)).join("\n");
  const failed = envs.includes("lint");
  steps.push([20, `${summary}\n  ${failed ? "evaluation failed :( (7.40 seconds)" : "congratulations :) (6.20 seconds)"}\n`], [10, { code: failed ? 1 : 0 }]);
  return steps;
}

const COBERTURA = `<?xml version="1.0" ?>
<coverage version="7.6.1" timestamp="1761471000000" lines-valid="27" lines-covered="25" line-rate="0.9259" branches-covered="0" branches-valid="0" branch-rate="0" complexity="0">
  <sources><source>${ROOT}/src</source></sources>
  <packages><package name="shop_api" line-rate="0.9259" branch-rate="0" complexity="0"><classes>
    <class name="pricing.py" filename="shop_api/pricing.py" complexity="0" line-rate="0.8889" branch-rate="0"><methods/><lines>
      <line number="2" hits="1"/><line number="3" hits="1"/><line number="6" hits="1"/><line number="8" hits="3"/><line number="11" hits="1"/>
      <line number="12" hits="1"/><line number="13" hits="1"/><line number="14" hits="0"/><line number="15" hits="0"/><line number="18" hits="1"/>
      <line number="19" hits="1"/><line number="20" hits="1"/><line number="22" hits="1"/><line number="23" hits="1"/><line number="25" hits="1"/>
      <line number="26" hits="2"/><line number="27" hits="2"/>
    </lines></class>
    <class name="settings.py" filename="shop_api/settings.py" complexity="0" line-rate="1" branch-rate="0"><methods/><lines>
      <line number="2" hits="1"/><line number="5" hits="1"/><line number="6" hits="1"/><line number="8" hits="1"/><line number="9" hits="1"/>
      <line number="10" hits="1"/><line number="11" hits="1"/><line number="14" hits="1"/>
    </lines></class>
  </classes></package></packages>
</coverage>
`;

// ---- Profile -------------------------------------------------------------------------------

type Row = [string, number, string, number, number, number, [number, number, number, number][]];

/** `scripts/report.py` under cProfile: (file, line, function, calls, own s, total s, callers). */
const PROFILE_ROWS: Row[] = [
  ["~", 0, "<built-in method builtins.exec>", 1, 0.0004, 1.9122, []],
  [`${ROOT}/scripts/report.py`, 1, "<module>", 1, 0.0021, 1.9118, [[0, 1, 0.0021, 1.9118]]],
  [`${ROOT}/scripts/report.py`, 12, "load_orders", 1, 0.0412, 1.6011, [[1, 1, 0.0412, 1.6011]]],
  [`${ROOT}/scripts/report.py`, 7, "parse_row", 18240, 0.4127, 0.9884, [[2, 18240, 0.4127, 0.9884]]],
  ["/usr/lib/python3.12/csv.py", 107, "__next__", 18241, 0.3271, 0.5715, [[2, 18241, 0.3271, 0.5715]]],
  ["~", 0, "<method 'title' of 'str' objects>", 18240, 0.2014, 0.2014, [[3, 18240, 0.2014, 0.2014]]],
  ["~", 0, "<method 'strip' of 'str' objects>", 18240, 0.0958, 0.0958, [[3, 18240, 0.0958, 0.0958]]],
  ["/usr/lib/python3.12/_pydecimal.py", 520, "__new__", 18240, 0.2785, 0.2785, [[3, 18240, 0.2785, 0.2785]]],
  ["~", 0, "<built-in method builtins.next>", 18241, 0.2444, 0.2444, [[4, 18241, 0.2444, 0.2444]]],
  [`${ROOT}/scripts/report.py`, 17, "totals_by_customer", 1, 0.1906, 0.2841, [[1, 1, 0.1906, 0.2841]]],
  ["~", 0, "<built-in method builtins.sorted>", 1, 0.0381, 0.0935, [[9, 1, 0.0381, 0.0935]]],
  [`${ROOT}/scripts/report.py`, 21, "<lambda>", 1412, 0.0554, 0.0554, [[10, 1412, 0.0554, 0.0554]]],
  ["~", 0, "<built-in method builtins.print>", 1412, 0.0244, 0.0244, [[1, 1412, 0.0244, 0.0244]]],
  ["~", 0, "<built-in method io.open>", 1, 0.0019, 0.0019, [[2, 1, 0.0019, 0.0019]]],
];

const profiles: ProfileInfo[] = [];
let profileSeq = 0;

function addProfile(name: string): ProfileInfo {
  const at = Date.now();
  const info: ProfileInfo = { id: `${at}-${++profileSeq}`, name, at, path: `~/.cache/forge/profiles/${pythonDemoName}/${at}.pstats` };
  profiles.unshift(info);
  return info;
}

registerMock("python_profiles", () => structuredClone(profiles));
registerMock("python_profile_view", async ({ id }) => {
  const info = profiles.find((p) => p.id === id);
  if (!info) throw new Error(`no profile ${String(id)}`);
  const view = await wasmCall<ProfileView>("python_demo_profile", { rows: PROFILE_ROWS });
  const rel = (f: string) => (f.startsWith(`${ROOT}/`) ? f.slice(ROOT.length + 1) : f);
  const mapTree = (n: ProfileView["callTree"][number]): ProfileView["callTree"][number] => ({ ...n, file: rel(n.file), children: n.children?.map(mapTree) });
  return { info, view: { ...view, hotSpots: view.hotSpots.map((h) => ({ ...h, file: rel(h.file) })), callTree: view.callTree.map(mapTree) } };
});
registerMock("python_profile_open", ({ path }) => addProfile(String(path).split("/").pop() ?? "snapshot"));
registerMock("python_profile_delete", ({ id }) => {
  const i = profiles.findIndex((p) => p.id === id);
  if (i >= 0) profiles.splice(i, 1);
});

// ---- debugpy -------------------------------------------------------------------------------

registerMock("python_debugpy_status", () => ({ installed: true, version: "1.8.7", interpreter: "Python 3.12 (shop-api)" }));
let debugPort = 5678;
registerMock("python_debug_plan", () => ({
  listenHost: "127.0.0.1",
  connectHost: "127.0.0.1",
  port: debugPort++,
  pathMappings: [],
  justMyCode: mockSettingValue("python.debugger.justMyCode") !== false,
  subProcess: mockSettingValue("python.debugger.subProcess") !== false,
}));

// ---- runs ----------------------------------------------------------------------------------

/** Inline helper scripts are long: show them shortened like the real console would wrap them. */
const shortArg = (a: string) => (a.startsWith("import base64;") ? "<forge_unittest.py>" : a);

async function pythonRun(r: RunRequest, runId: string): Promise<{ cmd: CommandLine; steps: Step[]; report?: string; flavor?: string; defaultFile?: string; coverage: boolean; profile: boolean } | null> {
  const cfg = r.configuration;
  if (!cfg || !isPythonDemo()) return null;
  const head = r.interpreterArgs ?? [];
  const debug = head.includes("debugpy");
  if (PY_TYPES.has(cfg.type)) {
    const c = await testCommand(cfg, runId, head, !!r.coverage);
    const program = c.program ?? (cfg.interpreter || PYTHON);
    const cmd = { program, args: c.args.map(shortArg), cwd: cfg.workingDirectory || "", env: { ...(c.env ?? {}), ...(cfg.env ?? {}) } };
    const steps =
      cfg.type === "pytest"
        ? debug
          ? [[200, `${bold("============================= test session starts ==============================")}\nplatform linux -- Python 3.12.7, pytest-8.3.3, pluggy-1.5.0\ncollected 1 item\n\n`] as Step]
          : pytestSteps(selected(c.args), !!r.coverage)
        : cfg.type === "unittest"
          ? unittestSteps(c.args)
          : toxSteps(c.args);
    return { cmd, steps, report: c.report, flavor: c.flavor, defaultFile: c.defaultFile, coverage: !!r.coverage, profile: false };
  }
  if (cfg.type !== "python" || (!debug && !r.profile)) return null;
  const o = cfg.options ?? {};
  const target = o.target === "module" ? ["-m", o.script ?? ""] : [o.script ?? ""];
  const profileFile = `~/.cache/forge/profiles/${pythonDemoName}/report.pstats`;
  const extra = r.profile ? ["-m", "cProfile", "-o", profileFile] : head;
  const args = [...(cfg.interpreterOptions ?? "").split(/\s+/).filter(Boolean), ...extra, ...target, ...(cfg.arguments ?? "").split(/\s+/).filter(Boolean)];
  const cmd = { program: cfg.interpreter || PYTHON, args, cwd: cfg.workingDirectory || "", env: { PYTHONUNBUFFERED: "1" } };
  const steps: Step[] = r.profile
    ? [
        [300, "Northwind Traders                    48211.30\nAlfreds Futterkiste                  41874.95\nAna Trujillo Emparedados             39102.10\n"],
        [200, "… 1409 more customers\n"],
        [100, { code: 0 }],
      ]
    : [[200, "Started shop-api in debug mode\n"]];
  return { cmd, steps, coverage: false, profile: !!r.profile };
}

registerMockRunHandler({
  commandLine: async (r) => (await pythonRun(r, r.runId ?? "preview"))?.cmd ?? null,
  start: async (r, runId) => {
    const run = await pythonRun(r, runId);
    if (!run) return null;
    const cfg = r.configuration!;
    const started = await startMockRun(runId, run.cmd, run.steps, {
      testRunner: !!r.testRunner,
      testReport: run.report,
      junitFlavor: run.flavor,
      defaultTestFile: run.defaultFile,
      configuration: PY_TYPES.has(cfg.type) ? cfg : undefined,
      coverage: run.coverage ? { name: cfg.name, text: () => COBERTURA, cwd: "src" } : undefined,
      exitEvent: run.profile ? async (): Promise<RunEvent> => ({ type: "profile", profile: addProfile(cfg.name) }) : undefined,
    });
    return started;
  },
});

// ---- Python Console ------------------------------------------------------------------------

interface MockConsole {
  vars: Map<string, ConsoleVariable>;
  count: number;
  output: number;
}
const consoles = new Map<string, MockConsole>();

function emitConsole(id: string, payload: unknown): void {
  emitMock(`python-console://${id}`, payload);
}

function print(id: string, text: string, color?: string): void {
  const lines: ConsoleLine[] = text
    .replace(/\n$/, "")
    .split("\n")
    .map((t) => ({ segments: [{ text: t, style: color ? { fg: color } : {} }] }));
  emitConsole(id, { type: "console", replaceLast: false, lines });
}

/** Canned results of the demo's console snippets. */
const RESULTS: [RegExp, (c: MockConsole) => string | null][] = [
  [/^cart\.total\(\)$/, () => "Decimal('29.75')"],
  [/^round_cents\(([\d.]+)\)$/, () => "2.67"],
  [/^len\((\w+)\.items\)$/, () => "1"],
  [/^settings$/, () => "Settings(database_url='sqlite:///./shop.db', host='127.0.0.1', port=8000, debug=False)"],
];

function setVar(c: MockConsole, name: string, type: string, value: string, children = false, size: number | null = null): void {
  c.vars.set(name, { name, type, value, size, expr: name, children });
}

function execute(id: string, code: string): void {
  const c = consoles.get(id);
  if (!c) return;
  for (const raw of code.split("\n")) {
    const line = raw.trim();
    if (!line || line.startsWith("#")) continue;
    const imp = /^from\s+([\w.]+)\s+import\s+(.+)$/.exec(line);
    if (imp) {
      for (const n of imp[2].split(",").map((x) => x.trim())) {
        if (n === "Decimal") setVar(c, n, "type", "<class 'decimal.Decimal'>", false);
        else if (/^[A-Z]/.test(n)) setVar(c, n, "type", `<class '${imp[1]}.${n}'>`);
        else setVar(c, n, "function", `<function ${n} at 0x7f3a2c1e5${(c.vars.size * 37).toString(16)}0>`);
      }
      continue;
    }
    if (/^import\s+/.test(line)) continue;
    const assign = /^(\w+)\s*=\s*(.+)$/.exec(line);
    if (assign) {
      const [, name, expr] = assign;
      if (/^Cart\(\)$/.test(expr)) setVar(c, name, "shop_api.pricing.Cart", "Cart(items=[])", true);
      else if (/^\d+$/.test(expr)) setVar(c, name, "int", expr);
      else if (/^["'].*["']$/.test(expr)) setVar(c, name, "str", `'${expr.slice(1, -1)}'`);
      else if (/^\[.*\]$/.test(expr)) setVar(c, name, "list", expr, true, expr.split(",").length);
      else setVar(c, name, "object", expr);
      continue;
    }
    const add = /^(\w+)\.add\((.+)\)$/.exec(line);
    if (add && c.vars.has(add[1])) {
      setVar(c, add[1], "shop_api.pricing.Cart", "Cart(items=[('mug', Decimal('12.50'), 2)])", true);
      continue;
    }
    const known = RESULTS.find(([re]) => re.test(line));
    const value = known ? known[1](c) : c.vars.get(line)?.value;
    if (value) print(id, c.vars.has("__ipython") ? `Out[${c.count}]: ${value}` : value);
    else if (/^print\((.*)\)$/.test(line)) print(id, /^print\(["'](.*)["']\)$/.exec(line)?.[1] ?? "");
    else print(id, `Traceback (most recent call last):\n  File "<console>", line 1, in <module>\nNameError: name '${line.split(/\W/)[0]}' is not defined`, "var(--danger)");
  }
  c.count += 1;
}

function sendVars(id: string): void {
  const c = consoles.get(id);
  if (!c) return;
  emitConsole(id, { type: "message", message: { type: "vars", vars: [...c.vars.values()].sort((a, b) => a.name.localeCompare(b.name)) } });
}

registerMock("python_console_start", ({ id }) => {
  const consoleId = String(id ?? `console-${consoles.size + 1}`);
  consoles.set(consoleId, { vars: new Map(), count: 1, output: 0 });
  setTimeout(() => {
    emitConsole(consoleId, {
      type: "message",
      message: { type: "ready", banner: "Python 3.12.7 (CPython) on linux", prompt: ">>> ", more: "... ", ipython: false },
    });
    sendVars(consoleId);
  }, 150);
  return { id: consoleId, commandLine: `${PYTHON} -u -c <forge_console.py>`, interpreter: "Python 3.12 (shop-api)" };
});

registerMock("python_console_send", ({ id, op, fields, echo }) => {
  const consoleId = String(id);
  if (!consoles.has(consoleId)) throw new Error(`no console ${consoleId}`);
  if (echo) print(consoleId, String(echo));
  const f = (fields ?? {}) as Record<string, unknown>;
  setTimeout(() => {
    if (op === "exec") {
      const code = String(f.code ?? "");
      const open = !f.block && /:\s*$/.test(code.split("\n").pop() ?? "");
      if (open) return emitConsole(consoleId, { type: "message", message: { type: "more", prompt: "... " } });
      execute(consoleId, code);
    } else if (op === "runfile") {
      print(consoleId, "seeded");
    } else if (op === "children") {
      const expr = String(f.expr);
      const vars: ConsoleVariable[] =
        expr === "cart"
          ? [{ name: "items", type: "list", value: "[('mug', Decimal('12.50'), 2)]", size: 1, expr: "cart.items", children: true }]
          : expr === "cart.items"
            ? [{ name: "0", type: "tuple", value: "('mug', Decimal('12.50'), 2)", size: 3, expr: "cart.items[0]", children: false }]
            : [];
      return emitConsole(consoleId, { type: "message", message: { type: "children", expr, vars } });
    }
    emitConsole(consoleId, { type: "message", message: { type: "done", prompt: ">>> " } });
    sendVars(consoleId);
  }, 60);
});

registerMock("python_console_stop", ({ id }) => {
  consoles.delete(String(id));
  emitConsole(String(id), { type: "exited", code: 0 });
});

let history: string[] = ["from shop_api.settings import settings", "settings"];
registerMock("python_console_history", () => [...history]);
registerMock("python_console_history_add", ({ entry }) => {
  history = [...history.filter((h) => h !== entry), String(entry)];
  return [...history];
});
