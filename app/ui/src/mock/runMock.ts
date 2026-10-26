/**
 * Browser-mode run backend: configurations live in memory (never in a real project) and
 * runs replay canned output, including PHPUnit TeamCity messages with one failing test.
 * Console lines and the test tree are computed by the Rust `forge-output` crate (WebAssembly),
 * exactly like the desktop backend.
 */
import { emitMock, registerMock } from "@/ipc";
import type { CommandLine, RunConfiguration, RunConfigurations, RunEvent, RunRequest } from "@/ipc/run";
import { decodeInterpreters, findInterpreter, GLOBAL_KEY, PROJECT_KEY } from "@/features/php/interpreters";
import { callWasmSync, forgeWasm, type ForgeWasm } from "@/wasm";
import { DEMO_COMPOSE_VOLUMES } from "./phpInterpretersMock";
import { demoProjectName } from "./demoProject";
import { isSymfonyDemo, symfonyDemoName } from "./demoSymfonyProject";
import { isWordPressDemo, wordpressDemoName } from "./demoWordPressProject";
import { drupalDemoName, isDrupalDemo } from "./demoDrupalProject";
import { webDemo } from "./demoWebProjects";
import { isLuaDemo, luaDemoName } from "./demoLuaProject";
import { isPythonDemo, pythonDemoName } from "./demoPythonProject";
import { isJavaDemo, javaDemoName } from "./demoJavaProject";
import { scratchCommandLine, scratchRunSteps } from "./scratchesMock";
import { mockSettingValue } from "./settingsMock";
import { frameworkCommand, frameworkSteps, recordTestRun, specFor, TEST_TYPES, testCommandLine, testSteps } from "./testFrameworksMock";
import { addMockCoverage, demoClover, demoLcov } from "./coverageMock";

const ROOT = `~/Projects/${demoProjectName}`;
export const projectRoot = () =>
  `~/Projects/${isSymfonyDemo() ? symfonyDemoName : isWordPressDemo() ? wordpressDemoName : isDrupalDemo() ? drupalDemoName : isLuaDemo() ? luaDemoName : isPythonDemo() ? pythonDemoName : isJavaDemo() ? javaDemoName : (webDemo()?.name ?? demoProjectName)}`;

/** The Lua demo (`?project=lua`): the LÖVE game and its build script. */
const LUA_CONFIGURATIONS: RunConfigurations = {
  selected: "pixel-dungeon",
  configurations: [
    { name: "pixel-dungeon", type: "lua-script", storeAsProjectFile: true, interpreter: "love", options: { file: "." } },
    { name: "build", type: "lua-script", options: { file: "tools/build.lua" } },
  ],
};

const PYTHON_CONFIGURATIONS: RunConfigurations = {
  selected: "shop-api",
  configurations: [
    { name: "shop-api", type: "python", storeAsProjectFile: true, workingDirectory: "src", options: { target: "module", script: "uvicorn" }, arguments: "shop_api.main:app --reload" },
    { name: "seed", type: "python", options: { target: "script", script: "scripts/seed.py" } },
  ],
};

let stored: RunConfigurations = isLuaDemo() ? LUA_CONFIGURATIONS : isPythonDemo() ? PYTHON_CONFIGURATIONS : {
  selected: "UserTest",
  configurations: [
    {
      name: "UserTest",
      type: "phpunit",
      storeAsProjectFile: true,
      env: { APP_ENV: "testing" },
      options: { scope: "class", class: "Tests\\Unit\\UserTest", file: "tests/Unit/UserTest.php" },
    },
    { name: "All tests", type: "phpunit", storeAsProjectFile: true, options: { scope: "config" } },
    { name: "artisan serve", type: "artisan", options: { command: "serve" } },
    { name: "artisan migrate", type: "artisan", options: { command: "migrate --seed" }, env: { DB_CONNECTION: "sqlite" } },
    { name: "dev", type: "npm", options: { packageJson: "package.json", command: "run", script: "dev", packageManager: "npm" } },
    {
      name: "Clear caches",
      type: "shell",
      options: { mode: "text", scriptText: "php artisan optimize:clear" },
    },
    { name: "localhost:8080", type: "php-builtin-server", options: { host: "localhost", port: "8080", documentRoot: "public", openBrowser: "false" } },
    { name: "Order page", type: "php-web-page", options: { server: "acme-shop-test", startUrl: "/orders/1042" } },
  ],
};

/** The Node.js demo (`?project=node`): the API server, a browser, attach and its tests. */
const NODE_CONFIGURATIONS: RunConfigurations = {
  selected: "server",
  configurations: [
    { name: "server", type: "node", storeAsProjectFile: true, interpreterOptions: "--import tsx", env: { PORT: "3000" }, options: { file: "src/server.ts" } },
    { name: "Order 1042 in Chrome", type: "javascript-debug", options: { url: "http://localhost:3000/orders/1042/total" } },
    { name: "Attach to server", type: "js-attach", options: { debugger: "node", host: "localhost", port: "9229" } },
    { name: "Unit tests", type: "vitest", storeAsProjectFile: true, options: { scope: "all" } },
    { name: "E2E tests", type: "playwright", options: { scope: "all" } },
  ],
};

// The WebForge demos start without configurations; npm scripts add temporary ones.
if (webDemo()) stored = webDemo()?.name === "orders-api" ? NODE_CONFIGURATIONS : { configurations: [] };

const clone = <T>(v: T): T => JSON.parse(JSON.stringify(v)) as T;

registerMock("run_configurations_load", () => clone(stored));
registerMock("run_configurations_save", ({ configurations }) => {
  stored = clone(configurations as RunConfigurations);
});

const split = (s: string | undefined) => (s ?? "").split(/\s+/).filter(Boolean);

/** Same command lines as the Rust kind registry, for previews and the run header. */
export function mockCommandLine(cfg: RunConfiguration): CommandLine {
  const o = cfg.options ?? {};
  const php = cfg.interpreter || "/usr/bin/php";
  const cwd = cfg.workingDirectory || "";
  switch (cfg.type) {
    case "scratch":
      return scratchCommandLine(cfg);
    case "python": {
      const python = cfg.interpreter || `${projectRoot()}/.venv/bin/python`;
      const target = o.target === "module" ? ["-m", o.script ?? ""] : [o.script ?? ""];
      return { program: python, args: [...split(cfg.interpreterOptions), ...target, ...split(cfg.arguments)], cwd, env: { PYTHONUNBUFFERED: "1" } };
    }
    case "php-script":
      return { program: php, args: [...split(cfg.interpreterOptions), o.file ?? "", ...split(cfg.arguments)], cwd };
    case "php-builtin-server": {
      const root = projectRoot();
      const docroot = o.documentRoot ? (o.documentRoot.startsWith("/") ? o.documentRoot : `${root}/${o.documentRoot}`) : root;
      const router = o.useRouter === "true" && o.routerScript ? [o.routerScript.startsWith("/") ? o.routerScript : `${root}/${o.routerScript}`] : [];
      const address = `${o.host || "localhost"}:${o.port || "8080"}`;
      return { program: php, args: [...split(cfg.interpreterOptions), "-S", address, "-t", docroot, ...router], cwd };
    }
    case "phpunit": {
      const args = [...split(cfg.interpreterOptions), "vendor/bin/phpunit", "--teamcity", "--configuration", o.configFile || "phpunit.xml"];
      args.push(...split(o.testRunnerOptions));
      const short = (o.class ?? "").split("\\").pop();
      if (o.scope === "directory") args.push(o.directory ?? "");
      if (o.scope === "file") args.push(o.file ?? "");
      if (o.scope === "class") args.push("--filter", `/${short}(::.*)?$/`, o.file ?? "");
      if (o.scope === "method") args.push("--filter", `/${short}::${o.method}( .*)?$/`, o.file ?? "");
      return { program: php, args: args.filter(Boolean), cwd };
    }
    case "artisan":
      return { program: php, args: ["artisan", ...split(o.command), ...split(cfg.arguments), "--ansi"], cwd };
    case "composer":
      return { program: "composer", args: ["run-script", o.script ?? "", "--ansi"], cwd };
    case "composer-command":
      return { program: "composer", args: [...split(o.command), ...split(cfg.arguments), "--ansi", "--no-interaction"], cwd };
    case "npm":
      return {
        program: o.packageManager || "npm",
        args: [o.command || "run", ...(o.command && o.command !== "run" ? [] : [o.script ?? ""]), ...split(cfg.arguments)],
        cwd,
      };
    case "node":
      return { program: cfg.interpreter || "node", args: [...split(cfg.interpreterOptions), o.file ?? "", ...split(cfg.arguments)].filter(Boolean), cwd };
    case "lua-script":
      return { program: cfg.interpreter || "lua", args: [...split(cfg.interpreterOptions), o.file ?? "", ...split(cfg.arguments)].filter(Boolean), cwd };
    case "shell":
      return o.mode === "text"
        ? { program: cfg.interpreter || "/bin/sh", args: ["-c", o.scriptText ?? ""], cwd }
        : { program: cfg.interpreter || "/bin/sh", args: [o.scriptPath ?? "", ...split(cfg.arguments)], cwd };
    default:
      return { program: cfg.interpreter || cfg.type, args: [o.file ?? "", ...split(cfg.arguments)].filter(Boolean), cwd };
  }
}

const quote = (a: string) => (/^[\w./:=@%+,-]+$/.test(a) ? a : `'${a.replace(/'/g, "'\\''")}'`);
const display = (c: CommandLine) => [c.program, ...c.args].map(quote).join(" ");

/** Configuration types whose program is PHP (the desktop backend's `PHP_KINDS`). */
const PHP_KINDS = new Set(["php-script", "phpunit", "pest", "codeception", "behat", "phpspec", "artisan", "php-builtin-server"]);

interface Wrapped {
  cmd: CommandLine;
  pathMappings: { remote: string; local: string }[];
}

/**
 * A PHP configuration's command line through its named interpreter (Docker, Compose, SSH,
 * WSL), built by the Rust `phpforge-interpreters` crate like in the desktop app.
 */
export async function throughInterpreter(cfg: RunConfiguration | undefined, cmd: CommandLine, env: Record<string, string> = {}): Promise<Wrapped> {
  if (!cfg || !PHP_KINDS.has(cfg.type)) return { cmd, pathMappings: [] };
  const list = decodeInterpreters(mockSettingValue(GLOBAL_KEY) ?? [], mockSettingValue(PROJECT_KEY) ?? []);
  const selected = cfg.interpreter || String(mockSettingValue("php.interpreter") ?? "php");
  const named = findInterpreter(list, selected);
  if (!named) return { cmd, pathMappings: [] };
  const root = projectRoot();
  const wasm = await forgeWasm();
  const out = callWasmSync<{ program: string; args: string[]; cwd: string; pathMappings: Wrapped["pathMappings"] }>(wasm.php_interpreter_wrap, {
    global: mockSettingValue(GLOBAL_KEY),
    project: mockSettingValue(PROJECT_KEY),
    selected,
    program: { type: "php" },
    args: cmd.args,
    cwd: cmd.cwd ? (cmd.cwd.startsWith("/") || cmd.cwd.startsWith("~") ? cmd.cwd : `${root}/${cmd.cwd}`) : root,
    env: { ...(cfg.env ?? {}), ...(cmd.env ?? {}), ...env },
    root,
    composeDir: root,
    volumes: DEMO_COMPOSE_VOLUMES[named.service] ?? [],
  });
  return { cmd: { program: out.program, args: out.args, cwd: cmd.cwd }, pathMappings: out.pathMappings };
}

/** The command line of a configuration; test frameworks are built by Rust (WebAssembly). */
async function commandLineOf(cfg: RunConfiguration, coverage = false): Promise<CommandLine> {
  if (specFor(cfg.type)) {
    const c = await frameworkCommand(cfg, coverage);
    return { program: c.program, args: c.args, cwd: cfg.workingDirectory || "", env: { ...(c.env ?? {}), ...(cfg.env ?? {}) } };
  }
  const cmd = TEST_TYPES.has(cfg.type) ? (({ report: _, ...c }) => c)(await testCommandLine(cfg)) : mockCommandLine(cfg);
  // PHPUnit and Pest write Clover XML with Xdebug's coverage mode.
  if (coverage && (cfg.type === "phpunit" || cfg.type === "pest"))
    return { ...cmd, args: [...cmd.args, "--coverage-clover", "/tmp/forge-test-reports/run-coverage/clover.xml"], env: { ...(cmd.env ?? {}), XDEBUG_MODE: "coverage" } };
  return cmd;
}

/**
 * A command on the configuration's run target ("Run on"), built by the Rust
 * `forge-runtargets` crate like in the desktop app; `null` for local runs.
 */
async function onTarget(cfg: RunConfiguration | undefined, cmd: CommandLine, runOn = cfg?.runOn): Promise<Wrapped | null> {
  if (!runOn || runOn.kind === "local") return null;
  const root = projectRoot();
  const wasm = await forgeWasm();
  const php = !!cfg && PHP_KINDS.has(cfg.type);
  const out = callWasmSync<{ program: string; args: string[]; pathMappings: Wrapped["pathMappings"] }>(wasm.run_target_wrap, {
    target: runOn,
    invocation: {
      program: php ? "php" : cmd.program,
      args: cmd.args,
      cwd: cmd.cwd ? (cmd.cwd.startsWith("/") || cmd.cwd.startsWith("~") ? cmd.cwd : `${root}/${cmd.cwd}`) : root,
      env: { ...(cfg?.env ?? {}), ...(cmd.env ?? {}) },
    },
    root,
    compose: runOn.kind === "composeService" ? [root, DEMO_COMPOSE_VOLUMES[runOn.service ?? ""] ?? [".:/var/www/html"]] : null,
    docker: "",
  });
  return { cmd: { program: out.program, args: out.args, cwd: cmd.cwd }, pathMappings: out.pathMappings };
}

/** A pack's own canned runs (PyForge's tests, debug and profile runs). */
export interface MockRunHandler {
  /** The command line of a configuration the handler runs; null for others. */
  commandLine: (r: RunRequest) => Promise<CommandLine | null>;
  /** Start the run; null when the handler does not run it. */
  start: (r: RunRequest, runId: string) => Promise<{ runId: string; commandLine: string } | null>;
}
const runHandlers: MockRunHandler[] = [];
export function registerMockRunHandler(h: MockRunHandler): void {
  runHandlers.push(h);
}

registerMock("run_target_info", async ({ configuration }) => {
  const cfg = configuration as RunConfiguration;
  if (!cfg.runOn || cfg.runOn.kind === "local") return null;
  const root = projectRoot();
  const wasm = await forgeWasm();
  return callWasmSync<{ debug: unknown }>(wasm.run_target_wrap, {
    target: cfg.runOn,
    invocation: { program: "true", cwd: root },
    root,
    docker: "",
  }).debug;
});

registerMock("run_command_line", async ({ request }) => {
  const r = request as RunRequest;
  for (const h of runHandlers) {
    const cmd = await h.commandLine(r);
    if (cmd) return cmd;
  }
  if (r.command) return (await onTarget(undefined, r.command, r.runOn))?.cmd ?? r.command;
  const local = await commandLineOf(r.configuration!, !!r.coverage);
  return (await onTarget(r.configuration, local))?.cmd ?? (await throughInterpreter(r.configuration, local)).cmd;
});

interface MockRun {
  timers: ReturnType<typeof setTimeout>[];
  done: boolean;
  /** Run with Coverage: the report read when the run exits. */
  coverage?: MockCoverage;
  /** Another event before `exited` (a profiler snapshot). */
  exitEvent?: () => Promise<RunEvent | null>;
  /** Handle of the run's `RunOutput` in the WebAssembly module. */
  output: number;
  /** Test runs: the configuration stored in the Test History. */
  configuration?: RunConfiguration;
  startedAt: number;
}
const runs = new Map<string, MockRun>();
let seq = 0;
let wasm: ForgeWasm | undefined;

/** Emit a raw event and what `forge-output` renders from it (console lines, test nodes). */
function emitRun(runId: string, e: RunEvent): void {
  const run = runs.get(runId);
  const input =
    e.type === "started"
      ? { type: "started", commandLine: e.commandLine }
      : e.type === "output"
        ? { type: "output", stream: e.stream, text: e.text }
        : e.type === "exited"
          ? { type: "exited", code: e.code, signal: e.signal }
          : null;
  const rendered: RunEvent[] = run && input && wasm ? JSON.parse(wasm.run_output_handle(JSON.stringify({ handle: run.output, input }))) : [];
  if (e.type !== "exited") emitMock(`run://${runId}`, e);
  rendered.forEach((r) => emitMock(`run://${runId}`, r));
  if (e.type === "exited") {
    if (run?.configuration && wasm) {
      const exported = JSON.parse(wasm.run_output_export(JSON.stringify({ handle: run.output })));
      if (exported) recordTestRun(run.configuration, run.startedAt, exported);
    }
    emitMock(`run://${runId}`, e);
  }
}

/** A JUnit report the runner wrote (Codeception, Behat), read before the exit. */
function emitReport(runId: string, xml: string): void {
  const run = runs.get(runId);
  if (!run || !wasm) return;
  const rendered: RunEvent[] = JSON.parse(wasm.run_output_handle(JSON.stringify({ handle: run.output, input: { type: "report", xml } })));
  rendered.forEach((r) => emitMock(`run://${runId}`, r));
}

/** The canned report of a Run with Coverage run. */
export interface MockCoverage {
  name: string;
  text: () => string;
  cwd?: string;
  rules?: { pattern: string; replacement: string }[];
}

/** Read the run's coverage report into a suite, then report the exit (like the backend). */
async function exitWithCoverage(runId: string, code: number): Promise<void> {
  const run = runs.get(runId);
  if (run?.coverage) {
    const root = projectRoot();
    const suite = await addMockCoverage(run.coverage.name, run.coverage.text(), {
      root,
      project: root.split("/").pop() ?? "",
      cwd: run.coverage.cwd,
      rules: run.coverage.rules,
    }).catch(() => null);
    if (suite) emitMock(`run://${runId}`, { type: "coverage", suite });
  }
  const extra = await run?.exitEvent?.();
  if (extra) emitMock(`run://${runId}`, extra);
  emitRun(runId, { type: "exited", code, signal: null });
}

export type Step =
  | [delayMs: number, text: string, stream?: "stdout" | "stderr"]
  | [delayMs: number, exit: { code: number }]
  | [delayMs: number, report: { report: string }];

function play(runId: string, steps: Step[]): void {
  const run = runs.get(runId)!;
  const emit = (e: RunEvent) => emitRun(runId, e);
  let at = 0;
  for (const step of steps) {
    at += step[0];
    run.timers.push(
      setTimeout(() => {
        if (run.done) return;
        if (typeof step[1] === "string") emit({ type: "output", stream: step[2] ?? "stdout", text: step[1] });
        else if ("report" in step[1]) emitReport(runId, step[1].report);
        else {
          run.done = true;
          void exitWithCoverage(runId, step[1].code);
        }
      }, at),
    );
  }
}

const esc = (s: string) => s.replace(/\|/g, "||").replace(/'/g, "|'").replace(/\n/g, "|n").replace(/\[/g, "|[").replace(/]/g, "|]");
const tc = (name: string, attrs: Record<string, string | number>) =>
  `##teamcity[${name} ${Object.entries(attrs)
    .map(([k, v]) => `${k}='${esc(String(v))}'`)
    .join(" ")} flowId='4242']\n`;

interface DemoTest {
  name: string;
  ms: number;
  fail?: { message: string; expected: string; actual: string; line: number };
  skip?: string;
}
interface DemoClass {
  fqcn: string;
  file: string;
  suite: "Unit" | "Feature";
  tests: DemoTest[];
}

const DEMO_TESTS: DemoClass[] = [
  {
    fqcn: "Tests\\Unit\\UserTest",
    file: "tests/Unit/UserTest.php",
    suite: "Unit",
    tests: [
      { name: "test_full_name_joins_first_and_last_name", ms: 4 },
      {
        name: "test_display_name_prefers_nickname",
        ms: 7,
        fail: { message: "Failed asserting that two strings are identical.", expected: "'Amazing Grace'", actual: "'Grace'", line: 22 },
      },
      { name: "email_is_normalized", ms: 2 },
      { name: "test_orders_relation_is_empty_for_new_users", ms: 38 },
    ],
  },
  {
    fqcn: "Tests\\Unit\\CheckoutTest",
    file: "tests/Unit/CheckoutTest.php",
    suite: "Unit",
    tests: [
      { name: "test_total_includes_tax", ms: 3 },
      { name: "test_empty_cart_cannot_be_checked_out", ms: 1, skip: "Cart rules move to the Cart service." },
    ],
  },
  {
    fqcn: "Tests\\Feature\\OrderTest",
    file: "tests/Feature/OrderTest.php",
    suite: "Feature",
    tests: [{ name: "test_user_can_view_order", ms: 112 }],
  },
];

function phpunitSteps(cfg: RunConfiguration): Step[] {
  const o = cfg.options ?? {};
  let classes = DEMO_TESTS;
  if (o.scope === "class" || o.scope === "file") classes = DEMO_TESTS.filter((c) => c.file === o.file || c.fqcn === o.class);
  if (o.scope === "method")
    classes = DEMO_TESTS.filter((c) => c.fqcn === o.class || c.file === o.file).map((c) => ({
      ...c,
      tests: c.tests.filter((t) => t.name === o.method),
    }));
  if (o.scope === "directory") classes = DEMO_TESTS.filter((c) => c.file.startsWith(`${(o.directory ?? "").replace(/\/$/, "")}/`));
  const filter = /--filter '\/::\(([^)]*)\)/.exec(o.testRunnerOptions ?? "");
  if (filter) {
    const names = filter[1].split("|");
    classes = classes.map((c) => ({ ...c, tests: c.tests.filter((t) => names.includes(t.name)) })).filter((c) => c.tests.length);
  }
  const all = classes.flatMap((c) => c.tests);
  const steps: Step[] = [
    [60, "PHPUnit 11.5.2 by Sebastian Bergmann and contributors.\n\nRuntime:       PHP 8.3.12\nConfiguration: " + ROOT + "/phpunit.xml\n\n"],
    [30, tc("testCount", { count: all.length })],
  ];
  const grouped = o.scope === "config" || !o.scope || o.scope === "directory";
  let currentSuite = "";
  for (const c of classes) {
    if (grouped && currentSuite !== c.suite) {
      if (currentSuite) steps.push([5, tc("testSuiteFinished", { name: currentSuite })]);
      currentSuite = c.suite;
      steps.push([5, tc("testSuiteStarted", { name: c.suite })]);
    }
    const abs = `${ROOT}/${c.file}`;
    steps.push([10, tc("testSuiteStarted", { name: c.fqcn, locationHint: `php_qn://${abs}::\\${c.fqcn}` })]);
    for (const t of c.tests) {
      const hint = `php_qn://${abs}::\\${c.fqcn}::${t.name}`;
      if (t.skip) {
        steps.push([20, tc("testIgnored", { name: t.name, message: t.skip, duration: t.ms, locationHint: hint })]);
        continue;
      }
      steps.push([15, tc("testStarted", { name: t.name, locationHint: hint })]);
      if (t.fail) {
        steps.push([
          t.ms * 12,
          tc("testFailed", {
            name: t.name,
            message: t.fail.message,
            details: ` ${abs}:${t.fail.line}\n`,
            type: "comparisonFailure",
            actual: t.fail.actual,
            expected: t.fail.expected,
            duration: t.ms,
          }),
        ]);
      }
      steps.push([t.ms * 12, tc("testFinished", { name: t.name, duration: t.ms })]);
    }
    steps.push([5, tc("testSuiteFinished", { name: c.fqcn })]);
  }
  if (grouped && currentSuite) steps.push([5, tc("testSuiteFinished", { name: currentSuite })]);
  const failed = all.filter((t) => t.fail);
  const skipped = all.filter((t) => t.skip);
  const time = all.reduce((s, t) => s + t.ms, 0) + 24;
  let summary = `\nTime: 00:00.${String(time).padStart(3, "0")}, Memory: 28.00 MB\n\n`;
  if (failed.length) {
    summary += `There was ${failed.length} failure:\n\n`;
    failed.forEach((t, i) => {
      const c = classes.find((x) => x.tests.includes(t))!;
      summary += `${i + 1}) ${c.fqcn}::${t.name}\n${t.fail!.message}\n--- Expected\n+++ Actual\n@@ @@\n-${t.fail!.expected}\n+${t.fail!.actual}\n\n${ROOT}/${c.file}:${t.fail!.line}\n\n`;
    });
    summary += `\x1b[37;41mFAILURES!\x1b[0m\n\x1b[37;41mTests: ${all.length}, Assertions: ${all.length + 3}, Failures: ${failed.length}${skipped.length ? `, Skipped: ${skipped.length}` : ""}.\x1b[0m\n`;
  } else {
    summary += `\x1b[30;42mOK (${all.length} tests, ${all.length + 3} assertions)\x1b[0m\n`;
  }
  steps.push([40, summary], [10, { code: failed.length ? 1 : 0 }]);
  return steps;
}

/** Output of a package.json script in a WebForge demo: the script line, then a dev server or a quick finish. */
function webNpmSteps(cfg: RunConfiguration): Step[] {
  const o = cfg.options ?? {};
  let pkg: { name?: string; version?: string; scripts?: Record<string, string> } = {};
  try {
    pkg = JSON.parse(webDemo()!.files[o.packageJson || "package.json"] ?? "{}");
  } catch {
    // An unsaved, broken package.json still runs.
  }
  const script = o.script ?? "";
  const command = pkg.scripts?.[script] ?? script;
  const header = `\n> ${pkg.name ?? "app"}@${pkg.version ?? "0.0.0"} ${script}\n> ${command}\n\n`;
  const debug = cfg.env?.NODE_OPTIONS
    ? "Debugger listening on ws://127.0.0.1:9229/3f1c2d8e-5b7a-4e0f-9a61-2c4d7b8e1f30\nFor help, see: https://nodejs.org/en/docs/inspector\n\n"
    : "";
  if (/^(vite|ng serve|nuxt dev)/.test(command))
    return [
      [150, header + debug],
      [500, "  \x1b[32m\x1b[1mVITE\x1b[22m v6.3.5\x1b[39m  ready in \x1b[1m388\x1b[22m ms\n\n"],
      [20, "  \x1b[32m➜\x1b[39m  \x1b[1mLocal\x1b[22m:   \x1b[36mhttp://localhost:\x1b[1m5173\x1b[22m/\x1b[39m\n"],
      [10, "  \x1b[32m➜\x1b[39m  \x1b[2mNetwork\x1b[22m: use \x1b[1m--host\x1b[22m to expose\n"],
    ];
  return [
    [150, header + debug],
    [400, `\x1b[32m✓\x1b[39m ${script} finished in 1.82s\n`],
    [20, { code: 0 }],
  ];
}

function stepsFor(cfg: RunConfiguration | undefined, cmd: CommandLine): Step[] {
  // Commands built in the UI (External Tools, HTTP requests) arrive without a configuration.
  if (!cfg) return externalToolSteps(cmd);
  const o = cfg.options ?? {};
  if (TEST_TYPES.has(cfg.type)) return testSteps(cfg);
  switch (cfg.type) {
    case "phpunit":
      return phpunitSteps(cfg);
    case "python":
      return pythonSteps(cfg);
    case "php-builtin-server": {
      const address = `${o.host || "localhost"}:${o.port || "8080"}`;
      const when = "[Thu Oct 22 09:14:02 2026]";
      return [
        [250, `${when} PHP 8.3.12 Development Server (http://${address}) started\n`],
        [1200, `${when} 127.0.0.1:51034 Accepted\n`],
        [40, `${when} 127.0.0.1:51034 [200]: GET /orders/1042\n`],
        [10, `${when} 127.0.0.1:51034 Closing\n`],
      ];
    }
    case "artisan":
      if ((o.command ?? "").startsWith("serve"))
        return [
          [300, "\n   \x1b[44;37m INFO \x1b[0m Server running on [\x1b[1mhttp://127.0.0.1:8000\x1b[0m].\n\n"],
          [20, "  \x1b[33mPress Ctrl+C to stop the server\x1b[0m\n\n"],
          [1500, "  \x1b[90m2026-10-20 09:14:02\x1b[0m /orders/1042 ......................... \x1b[90m~ 0.21ms\x1b[0m\n"],
          [700, "  \x1b[90m2026-10-20 09:14:03\x1b[0m /build/assets/app.js ................. \x1b[90m~ 0.04ms\x1b[0m\n"],
        ];
      if ((o.command ?? "").startsWith("migrate"))
        return [
          [200, "\n   \x1b[44;37m INFO \x1b[0m Running migrations.\n\n"],
          [300, "  2026_01_01_000000_create_orders_table \x1b[90m.................\x1b[0m \x1b[90m14.62ms\x1b[0m \x1b[32;1mDONE\x1b[0m\n\n"],
          [200, "   \x1b[44;37m INFO \x1b[0m Seeding database.\n\n"],
          [100, { code: 0 }],
        ];
      return [[200, `\n   \x1b[44;37m INFO \x1b[0m Command [${o.command}] finished.\n\n`], [50, { code: 0 }]];
    case "node":
      return [[350, "Orders API listening on http://localhost:3000\n"]];
    case "npm":
      if (webDemo()) return webNpmSteps(cfg);
      return [
        [150, `\n> dev\n> vite\n\n`],
        [500, "\n  \x1b[32m\x1b[1mVITE\x1b[22m v7.0.4\x1b[39m  ready in \x1b[1m412\x1b[22m ms\n\n"],
        [20, "  \x1b[32m➜\x1b[39m  \x1b[1mLocal\x1b[22m:   \x1b[36mhttp://localhost:\x1b[1m5173\x1b[22m/\x1b[39m\n"],
        [10, "  \x1b[32m➜\x1b[39m  \x1b[1mAPP_URL\x1b[22m: \x1b[36mhttp://acme-shop.test\x1b[39m\n\n"],
        [10, "  \x1b[90mLARAVEL\x1b[39m plugin v1.2.0\n"],
      ];
    case "shell":
      return [
        [200, "\n   \x1b[44;37m INFO \x1b[0m Clearing cached bootstrap files.\n\n"],
        [100, "  config \x1b[90m.......................................\x1b[0m \x1b[90m2.11ms\x1b[0m \x1b[32;1mDONE\x1b[0m\n"],
        [80, "  cache \x1b[90m........................................\x1b[0m \x1b[90m4.80ms\x1b[0m \x1b[32;1mDONE\x1b[0m\n"],
        [80, "  routes \x1b[90m.......................................\x1b[0m \x1b[90m0.97ms\x1b[0m \x1b[32;1mDONE\x1b[0m\n\n"],
        [50, { code: 0 }],
      ];
    case "scratch":
      return scratchRunSteps(cfg);
    case "composer-command":
      return composerSteps(split(o.command));
    case "lua-script":
      return luaSteps(cfg);
    case "external-tool":
      return externalToolSteps(cmd);
    case "php-script":
      return [
        [150, "Generating invoices for 3 orders…\n"],
        [300, "PHP Warning:  Undefined array key \"vat\" in " + ROOT + "/app/Services/InvoiceService.php on line 18\n", "stderr"],
        [100, "Done.\n"],
        [50, { code: 0 }],
      ];
    default:
      return [[100, "Done.\n"], [50, { code: 0 }]];
  }
}

const green = (s: string) => `\x1b[32m${s}\x1b[39m`;
const yellow = (s: string) => `\x1b[33m${s}\x1b[39m`;

/** Canned output of Composer commands (nothing is installed in browser mode). */
export function composerSteps(args: string[]): Step[] {
  const [command, ...rest] = args;
  const packages = rest.filter((a) => !a.startsWith("-"));
  const autoload = [
    [120, `${green("Generating optimized autoload files")}\n`],
    [200, "> Illuminate\\Foundation\\ComposerScripts::postAutoloadDump\n> @php artisan package:discover --ansi\n\n"],
    [150, "   \x1b[44;37m INFO \x1b[0m Discovering packages.\n\n  laravel/sail \x1b[90m........................................\x1b[0m \x1b[32;1mDONE\x1b[0m\n\n"],
  ] as Step[];
  switch (command) {
    case "install":
      return [
        [150, `${green("Installing dependencies from lock file (including require-dev)")}\nVerifying lock file contents can be installed on current platform.\n`],
        [200, "Nothing to install, update or remove\n"],
        ...autoload,
        [50, "No security vulnerability advisories found.\n"],
        [20, { code: 0 }],
      ];
    case "update":
    case "require": {
      const [pkg, constraint] = (packages[0] ?? "laravel/framework").split(":");
      const steps: Step[] = [];
      if (command === "require") steps.push([120, `${green(`./composer.json has been updated`)}\nRunning composer update ${pkg}\n`]);
      steps.push(
        [200, `${green("Loading composer repositories with package information")}\n${green("Updating dependencies")}\n`],
        [300, `Lock file operations: ${command === "require" ? "1 install" : "0 installs"}, ${command === "require" ? "0" : "1"} update, 0 removals\n`],
        [100, `  - ${command === "require" ? "Locking" : "Upgrading"} ${green(pkg)} (${yellow(command === "require" ? (constraint ?? "v4.0.8").replace(/^\^/, "v") + ".0" : "v12.1.0 => v12.3.0")})\n`],
        [100, `${green("Writing lock file")}\n${green("Installing dependencies from lock file (including require-dev)")}\n`],
        [250, `Package operations: ${command === "require" ? "1 install, 0 updates" : "0 installs, 1 update"}, 0 removals\n  - ${command === "require" ? "Installing" : "Upgrading"} ${green(pkg)}: Extracting archive\n`],
        ...autoload,
        [20, { code: 0 }],
      );
      return steps;
    }
    case "remove":
      return [
        [120, `${green("./composer.json has been updated")}\nRunning composer update ${packages[0] ?? ""}\n`],
        [250, `${green("Loading composer repositories with package information")}\n${green("Updating dependencies")}\nLock file operations: 0 installs, 0 updates, 1 removal\n  - Removing ${green(packages[0] ?? "")}\n`],
        [200, `${green("Writing lock file")}\nPackage operations: 0 installs, 0 updates, 1 removal\n`],
        ...autoload,
        [20, { code: 0 }],
      ];
    case "validate":
      return [[250, `${green("./composer.json is valid")}\n`], [20, { code: 0 }]];
    case "outdated":
      return [
        [200, `${green("Color legend:")}\n- ${yellow("patch or minor")} release available - update recommended\n- \x1b[31mmajor\x1b[39m release available - update possible\n\n`],
        [100, `${green("Direct dependencies required in composer.json:")}\nlaravel/framework v12.1.0 ${yellow("v12.3.0")} The Laravel Framework.\nphpunit/phpunit   11.4.0  \x1b[31m12.0.2\x1b[39m  The PHP Unit Testing framework.\n`],
        [20, { code: 0 }],
      ];
    default:
      return [[150, "Done.\n"], [20, { code: 0 }]];
  }
}

/** Canned output of the demo External Tools (PHPStan lines link through the tool's output filter). */
function externalToolSteps(cmd: CommandLine): Step[] {
  const program = cmd.program.split("/").pop() ?? cmd.program;
  if (program === "phpstan") {
    const file = cmd.args.find((a) => a.endsWith(".php")) ?? "app/Models/User.php";
    return [
      [300, `${file}:22:Method App\\Models\\User::fullName() has no return type specified.\n`],
      [40, `${file}:34:Method App\\Models\\User::totalSpent() should return float but returns int.\n`],
      [30, { code: 1 }],
    ];
  }
  if (program === "php-cs-fixer")
    return [
      [300, "Loaded config default from \"" + ROOT + "/.php-cs-fixer.dist.php\".\n"],
      [200, "   1) app/Models/User.php\n\nFixed 1 of 1 files in 0.042 seconds, 14.00 MB memory used\n"],
      [30, { code: 0 }],
    ];
  if (program === "uv" || program.startsWith("python")) {
    const verb = cmd.args.includes("uninstall") ? "Uninstalled" : "Installed";
    const names = cmd.args.filter((a) => !a.startsWith("-") && !a.includes("/") && !["pip", "install", "uninstall", "-m"].includes(a));
    const steps: Step[] = [
      [250, program === "uv" ? `Resolved ${names.length + 4} packages in 412ms\n` : `Collecting ${names.join(" ")}\n`],
      [300, program === "uv" ? `Prepared ${names.length} packages in 180ms\n` : `Downloading ${names[0] ?? "package"}.whl (153 kB)\n`],
      [200, `${verb} ${names.length} package${names.length === 1 ? "" : "s"} in 12ms\n`],
    ];
    for (const n of names) steps.push([20, ` ${verb === "Installed" ? "+" : "-"} ${n}\n`]);
    steps.push([40, { code: 0 }]);
    return steps;
  }
  if (program === "git")
    return [
      [150, "3f1c2d8e (Marlon 2026-09-12 10:41:07 +0200 19) class User extends Authenticatable\n"],
      [20, { code: 0 }],
    ];
  return [[100, `${cmd.program} finished.\n`], [30, { code: 0 }]];
}

/** Canned output of the Python demo: uvicorn keeps serving, scripts finish. */
function pythonSteps(cfg: RunConfiguration): Step[] {
  const o = cfg.options ?? {};
  if (o.target === "module" ? o.script === "uvicorn" : (o.script ?? "").endsWith("main.py"))
    return [
      [250, "\x1b[32mINFO\x1b[0m:     Will watch for changes in these directories: ['" + projectRoot() + "/src']\n"],
      [60, "\x1b[32mINFO\x1b[0m:     Uvicorn running on \x1b[1mhttp://127.0.0.1:8000\x1b[0m (Press CTRL+C to quit)\n"],
      [60, "\x1b[32mINFO\x1b[0m:     Started reloader process [48211] using \x1b[36m\x1b[1mWatchFiles\x1b[0m\n"],
      [300, "\x1b[32mINFO\x1b[0m:     Application startup complete.\n"],
      [1200, '\x1b[32mINFO\x1b[0m:     127.0.0.1:51034 - "\x1b[1mGET /orders/1042 HTTP/1.1\x1b[0m" \x1b[32m200 OK\x1b[0m\n'],
    ];
  return [
    [200, "seeded\n"],
    [40, { code: 0 }],
  ];
}

/** Canned output of the Lua demo: the LÖVE game keeps running, scripts finish. */
function luaSteps(cfg: RunConfiguration): Step[] {
  if ((cfg.interpreter ?? "").endsWith("love"))
    return [
      [250, "Pixel Dungeon — LÖVE 11.5 (Mysterious Mysteries), LuaJIT 2.1.1720049189\n"],
      [200, "Generated 48x32 dungeon (seed 1760951142, 7 rooms)\n"],
      [120, "Ada spawned at (4, 3) with 20 HP\n"],
      [900, "Picked up 15 gold at (9, 5)\n"],
      [700, "Error: src/world.lua:35: attempt to index field 'tiles' (a nil value)\nstack traceback:\n\tsrc/world.lua:35: in function 'isWalkable'\n\tmain.lua:28: in function 'update'\n\t[love \"callbacks.lua\"]:162: in function <[love \"callbacks.lua\"]:144>\n", "stderr"],
    ];
  return [
    [120, "$ mkdir -p dist\n"],
    [150, "$ zip -9 -r dist/pixel-dungeon.love main.lua conf.lua src lib assets\n"],
    [200, "  adding: main.lua (deflated 58%)\n  adding: conf.lua (deflated 31%)\n  adding: src/player.lua (deflated 61%)\n  adding: src/world.lua (deflated 57%)\n  adding: lib/vector.lua (deflated 55%)\n"],
    [100, "Built dist/pixel-dungeon.love\n"],
    [50, { code: 0 }],
  ];
}

/**
 * Start a canned run: `started`, then `steps`, rendered by `forge-output` like a real one.
 * Also used by other mocks that stream `run://` events (`cli_tools_run`).
 */
export async function startMockRun(
  runId: string,
  cmd: CommandLine,
  steps: Step[],
  opts: {
    testRunner?: boolean;
    pathMappings?: Wrapped["pathMappings"];
    outputFilters?: string[];
    testReport?: string;
    junitFlavor?: string;
    configuration?: RunConfiguration;
    locationRules?: { pattern: string; replacement: string }[];
    defaultTestFile?: string;
    coverage?: MockCoverage;
    exitEvent?: () => Promise<RunEvent | null>;
  } = {},
): Promise<{ runId: string; commandLine: string }> {
  wasm ??= await forgeWasm();
  const commandLine = display(cmd);
  const options = {
    testRunner: !!opts.testRunner,
    root: projectRoot(),
    cwd: cmd.cwd || undefined,
    cycleBufferKb: Number(mockSettingValue("run.console.cycleBufferKb") ?? 1024),
    pathMappings: opts.pathMappings ?? [],
    outputFilters: opts.outputFilters ?? [],
    testReport: opts.testReport ?? "teamCity",
    junitFlavor: opts.junitFlavor ?? "generic",
    locationRules: opts.locationRules ?? [],
    defaultTestFile: opts.defaultTestFile ?? null,
  };
  const old = runs.get(runId);
  if (old) wasm.run_output_drop(JSON.stringify({ handle: old.output }));
  runs.set(runId, {
    timers: [],
    done: false,
    output: Number(wasm.run_output_new(JSON.stringify(options))),
    configuration: opts.testRunner ? opts.configuration : undefined,
    startedAt: Date.now(),
    coverage: opts.coverage,
    exitEvent: opts.exitEvent,
  });
  const pid = 40000 + ++seq;
  setTimeout(() => emitRun(runId, { type: "started", pid, commandLine }), 0);
  play(runId, steps);
  return { runId, commandLine };
}

registerMock("run_start", async ({ request }) => {
  const r = request as RunRequest;
  const runId = r.runId ?? `mock-run-${++seq}`;
  for (const h of runHandlers) {
    const started = await h.start(r, runId);
    if (started) return started;
  }
  const cfg = r.command ? undefined : r.configuration;
  const spec = cfg ? specFor(cfg.type) : undefined;
  const framework = cfg && spec ? await frameworkCommand(cfg, !!r.coverage) : undefined;
  const test = cfg && TEST_TYPES.has(cfg.type) ? (await testCommandLine(cfg)).report : undefined;
  const local = r.command ?? (await commandLineOf(r.configuration!, !!r.coverage));
  const withArgs = { ...local, args: [...(r.interpreterArgs ?? []), ...local.args] };
  const { cmd, pathMappings } = (await onTarget(cfg, withArgs, cfg ? cfg.runOn : r.runOn)) ?? (await throughInterpreter(cfg, withArgs, r.env));
  const withEnv = r.configuration && r.env ? { ...r.configuration, env: { ...r.configuration.env, ...r.env } } : r.configuration;
  const steps = framework && withEnv ? frameworkSteps(withEnv) : stepsFor(withEnv, local);
  const recorded = cfg && (cfg.type === "phpunit" || test || framework) ? cfg : undefined;
  let coverage: MockCoverage | undefined;
  if (r.coverage && cfg) {
    if (framework?.coverage) coverage = { name: cfg.name, text: demoLcov, cwd: cfg.workingDirectory || undefined, rules: framework.locationRules };
    else if (cfg.type === "phpunit" || cfg.type === "pest") coverage = { name: cfg.name, text: () => demoClover(ROOT) };
  }
  return startMockRun(runId, cmd, steps, {
    testRunner: !!r.testRunner,
    pathMappings,
    outputFilters: r.outputFilters,
    testReport: framework?.report ?? test?.report,
    junitFlavor: framework?.flavor ?? test?.flavor,
    configuration: recorded,
    locationRules: framework?.locationRules,
    defaultTestFile: framework?.defaultFile,
    coverage,
  });
});

function stop(runId: string): void {
  const run = runs.get(String(runId));
  if (!run || run.done) return;
  run.done = true;
  run.timers.forEach(clearTimeout);
  setTimeout(() => emitRun(runId, { type: "exited", code: null, signal: 15 }), 30);
}

registerMock("run_stop", ({ runId }) => stop(String(runId)));
registerMock("run_dispose", ({ runId }) => {
  stop(String(runId));
  const run = runs.get(String(runId));
  // Drop after the final `exited` event was rendered.
  if (run) setTimeout(() => wasm?.run_output_drop(JSON.stringify({ handle: run.output })), 100);
  runs.delete(String(runId));
});
registerMock("run_write", ({ runId, data }) => {
  const run = runs.get(String(runId));
  if (!run || !wasm) return;
  const echo: RunEvent[] = JSON.parse(wasm.run_output_handle(JSON.stringify({ handle: run.output, input: { type: "input", text: String(data) } })));
  echo.forEach((e) => emitMock(`run://${runId}`, e));
});
registerMock("output_file_links", async ({ text }) => {
  wasm ??= await forgeWasm();
  return JSON.parse(wasm.output_file_links(JSON.stringify({ text, root: projectRoot() })));
});
