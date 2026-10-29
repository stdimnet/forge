/**
 * Browser-mode KotlinForge: the `kotlin_*` commands and Kotlin runs on the demo Gradle
 * project (`?project=kotlin`, demoKotlinProject.ts). Main functions, Kotest and JUnit
 * discovery, command lines, ktlint / detekt reports and coroutine dumps go through the Rust
 * KotlinForge crate (WebAssembly), like in the desktop app; installs, the ktlint and detekt
 * findings and the run output are canned. `?kotlin-lsp=missing` starts without kotlin-lsp so
 * the download offer can be tried. Nothing is downloaded or run.
 */
import { emitMock, mockHandler, registerMock } from "@/ipc";
import type { KotlinDebugPlan, KotlinInstallProgress, KotlinStatus, LintProblem } from "@/ipc/kotlin";
import type { CommandLine, RunConfiguration, RunRequest } from "@/ipc/run";
import { callWasm, callWasmSync, forgeWasm } from "@/wasm";
import { isKotlinDemo, KT_PKG, KT_TEST_PKG, kotlinDemoInfo, kotlinDemoName } from "./demoKotlinProject";
import { mockFiles } from "./workspaceMock";
import { mockSettingValue } from "./settingsMock";
import { projectRoot, startMockRun, type MockCoverage, type Step } from "./runMock";
import { markMockServerInstalled } from "./lspMock";
import "./kotlinDebugMock";

const HOME = "/home/anna";
const ROOT = () => projectRoot().replace(/^~/, HOME);
const PINNED = "263.6379.0";

let kotlinLsp: string | null =
  typeof location !== "undefined" && new URLSearchParams(location.search).get("kotlin-lsp") === "missing" ? null : PINNED;
let jars = true;

const setting = (key: string) => String(mockSettingValue(key) ?? "").trim();
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

if (isKotlinDemo()) {
  registerMock("project_detect", () => structuredClone(kotlinDemoInfo));
}

async function simulateInstall(id: KotlinInstallProgress["id"], steps: string[], download: string, total: number): Promise<void> {
  const emit = (event: KotlinInstallProgress["event"]) => emitMock("kotlin://install", { id, event } satisfies KotlinInstallProgress);
  emit({ kind: "step", message: steps[0] });
  for (let done = 0; done <= total; done += total / 8) {
    await sleep(120);
    emit({ kind: "progress", label: download, done: Math.round(done), total });
  }
  for (const s of steps.slice(1)) {
    await sleep(150);
    emit({ kind: "step", message: s });
  }
}

registerMock("kotlin_status", (): KotlinStatus => {
  const fwcd = setting("kotlin.lsp.server") === "kotlin-language-server";
  const managed = (name: string, version: string) => ({ source: "managed", path: `${HOME}/.local/share/forge/langservers/jars/${name}-${version}.jar` });
  return {
    server: fwcd ? "kotlin-language-server" : "kotlin-lsp",
    serverPath: fwcd ? "/usr/local/bin/kotlin-language-server" : kotlinLsp ? `${HOME}/.local/share/forge/langservers/kotlin-lsp/${kotlinLsp}/bin/intellij-server` : null,
    serverVersion: fwcd ? null : kotlinLsp,
    pinnedVersion: PINNED,
    downloadSize: 368_488_700,
    ktlint: jars ? managed("ktlint", "1.8.0") : null,
    detekt: jars ? managed("detekt", "1.23.8") : { source: "gradle", path: "" },
    kotlinc: "/usr/local/bin/kotlinc",
    problems: fwcd || kotlinLsp ? [] : ["kotlin-lsp is not installed"],
    remote: false,
  };
});

registerMock("kotlin_lsp_install", async () => {
  await simulateInstall(
    "kotlin-lsp",
    [
      `Downloading https://download.jetbrains.com/language-server/kotlin-server/${PINNED}/kotlin-server-${PINNED}.tar.gz…`,
      "Unpacking kotlin-lsp…",
      `Verified SHA-256 ab8ca4455dc2fc5fe1a24db2bccc46c104254d2c465155c4251ee65df8f3f7cc of kotlin-lsp ${PINNED}.`,
    ],
    `Downloading kotlin-lsp ${PINNED}`,
    368_488_700,
  );
  kotlinLsp = PINNED;
  markMockServerInstalled();
  return kotlinLsp;
});

registerMock("kotlin_linters_install", async () => {
  await simulateInstall("ktlint", ["Downloading ktlint-cli-1.8.0-all.jar…", "Verified SHA-256 of ktlint-1.8.0.jar."], "Downloading ktlint 1.8.0", 71_470_431);
  await simulateInstall("detekt", ["Downloading detekt-cli-1.23.8-all.jar…", "Verified SHA-256 of detekt-1.23.8.jar."], "Downloading detekt 1.23.8", 69_922_269);
  jars = true;
  return [`${HOME}/.local/share/forge/langservers/jars/ktlint-1.8.0.jar`, `${HOME}/.local/share/forge/langservers/jars/detekt-1.23.8.jar`];
});

registerMock("kotlin_main_functions", async ({ path, text }) => callWasm((await forgeWasm()).kotlin_main_functions, { path, text }));
registerMock("kotlin_coroutines", async ({ value }) => JSON.parse((await forgeWasm()).kotlin_coroutines(String(value ?? ""))));

// ---- ktlint and detekt ----------------------------------------------------------------------

/** What ktlint would say about a text: a few of its standard rules, as its JSON report. */
function ktlintReport(path: string, text: string): string {
  const errors: { line: number; column: number; message: string; rule: string }[] = [];
  text.split("\n").forEach((l, i) => {
    if (/^import .*\.\*$/.test(l)) errors.push({ line: i + 1, column: 1, message: "Wildcard import", rule: "standard:no-wildcard-imports" });
    if (/[ \t]+$/.test(l)) errors.push({ line: i + 1, column: l.trimEnd().length + 1, message: "Trailing space(s)", rule: "standard:no-trailing-spaces" });
    if (l.length > 120) errors.push({ line: i + 1, column: 121, message: "Exceeded max line length (120)", rule: "standard:max-line-length" });
    if (/\) \/\/ /.test(l)) errors.push({ line: i + 1, column: l.indexOf("//") + 1, message: "A comment in a 'value_argument_list' is only allowed when placed on a separate line", rule: "standard:value-argument-comment" });
  });
  return JSON.stringify([{ file: path, errors }]);
}

registerMock("kotlin_lint", async ({ path, content }): Promise<LintProblem[]> => {
  if (mockSettingValue("kotlin.ktlint.enabled") === false || !jars) return [];
  const p = String(path);
  const wasm = await forgeWasm();
  const found = callWasmSync<LintProblem[]>(wasm.kotlin_lint_report, { format: "ktlintJson", text: ktlintReport(p, String(content ?? "")) });
  return found.map((x) => ({ ...x, file: p }));
});

registerMock("kotlin_format", ({ content }) =>
  String(content ?? "")
    .split("\n")
    .map((l) => l.replace(/[ \t]+$/, ""))
    .join("\n"),
);

/** detekt's SARIF report of the demo (MagicNumber, a long function name rule). */
function detektSarif(): string {
  const result = (rule: string, level: string, message: string, file: string, line: number, column: number) => ({
    ruleId: `detekt.${rule}`,
    level,
    message: { text: message },
    locations: [{ physicalLocation: { artifactLocation: { uri: `file://${ROOT()}/${file}` }, region: { startLine: line, startColumn: column } } }],
  });
  const magic = "This expression contains a magic number. Consider defining it to a well named constant.";
  const results = [
    result("style.MagicNumber", "warning", magic, `${KT_PKG}/OrderService.kt`, 10, 47),
    result("style.MagicNumber", "warning", magic, `${KT_PKG}/Main.kt`, 7, 45),
    result("style.MagicNumber", "warning", magic, `${KT_PKG}/Main.kt`, 13, 19),
    result("style.MagicNumber", "warning", magic, `${KT_PKG}/Cart.kt`, 20, 50),
    result("style.WildcardImport", "warning", "kotlinx.coroutines.* is a wildcard import. Replace it with fully qualified imports.", `${KT_PKG}/Main.kt`, 3, 1),
  ];
  return JSON.stringify({ version: "2.1.0", runs: [{ tool: { driver: { name: "detekt", version: "1.23.8" } }, results }] });
}

registerMock("kotlin_detekt", async ({ paths }): Promise<LintProblem[]> => {
  const wasm = await forgeWasm();
  const all = callWasmSync<LintProblem[]>(wasm.kotlin_lint_report, { format: "sarif", text: detektSarif(), detekt: true }).map((p) => ({
    ...p,
    file: p.file.replace(`${ROOT()}/`, ""),
  }));
  const only = (paths as string[] | null) ?? [];
  await sleep(400);
  return only.length ? all.filter((p) => only.includes(p.file)) : all;
});

// ---- Debugging ------------------------------------------------------------------------------

registerMock("kotlin_debug_prepare", ({ configuration }): KotlinDebugPlan => {
  const cfg = configuration as RunConfiguration;
  const adapter = setting("kotlin.debug.debugger") === "java-debug" ? "java" : "kotlin";
  if (adapter === "kotlin" && !kotlinLsp) throw new Error("The Kotlin debugger runs in kotlin-lsp: install kotlin-lsp first");
  if (cfg.type === "jvm-remote-debug") return { adapter, dapPort: 41400, hostName: cfg.options?.host || "localhost", jdwpPort: Number(cfg.options?.port || 5005), run: false };
  const gradleTests = ["kotest", "junit", "testng", "gradle"].includes(cfg.type);
  return { adapter, dapPort: 41400, hostName: "127.0.0.1", jdwpPort: gradleTests ? 5005 : 47120, run: true };
});

// ---- Runs -------------------------------------------------------------------------------------

const DEBUG_ENV = "FORGE_JAVA_DEBUG_PORT";

const isKotlinRun = (cfg: RunConfiguration) =>
  cfg.type === "kotest" ||
  cfg.type === "kotlin-script" ||
  (cfg.type === "java-application" && cfg.options?.kotlin === "true") ||
  ((cfg.type === "junit" || cfg.type === "testng") && cfg.options?.language === "kotlin");

interface KotlinCommand {
  program: string;
  args: string[];
  env?: Record<string, string>;
  test?: { locationRules: { pattern: string; replacement: string }[]; defaultFile?: string; coverage?: { file: string; locationRules?: { pattern: string; replacement: string }[] } };
}

function buildRequest() {
  const files: Record<string, string> = {};
  for (const [path, text] of Object.entries(mockFiles)) if (!path.includes("/")) files[path] = text;
  return { folder: kotlinDemoName, files, exec: { useWrapper: mockSettingValue("java.gradle.useWrapper") !== false, buildDir: ROOT(), windows: false } };
}

async function kotlinCommand(cfg: RunConfiguration, coverage: boolean, debugPort: number | null): Promise<KotlinCommand> {
  const wasm = await forgeWasm();
  if (cfg.type === "junit" || cfg.type === "testng") {
    return callWasmSync<KotlinCommand>(wasm.java_command_line, { configuration: cfg, build: { ...buildRequest(), prefer: null }, coverage, java: "java", debugPort });
  }
  return callWasmSync<KotlinCommand>(wasm.kotlin_command_line, { configuration: cfg, build: buildRequest(), coverage, debugPort, runner: setting("kotlin.run.runner") || null });
}

interface DemoTest {
  cls: string;
  name: string;
  ms: number;
  fail?: string;
}

const SPEC = "com.acme.orders.CartSpec";
const JUNIT = "com.acme.orders.OrderServiceTest";
const TESTS: DemoTest[] = [
  { cls: SPEC, name: "adds items", ms: 0.018 },
  { cls: SPEC, name: "rejects empty quantities", ms: 0.006 },
  { cls: SPEC, name: "totals -- sums quantity times price", ms: 0.003 },
  { cls: SPEC, name: "totals -- applies the discount", ms: 0.004, fail: "expected:<950L> but was:<900L>" },
  { cls: JUNIT, name: "checks out an order()", ms: 0.112 },
  { cls: JUNIT, name: "gives ten percent off large orders()", ms: 0.105 },
];

const leaf = (name: string) => name.split(" -- ").pop()!.replace(/\(\)$/, "");
const esc = (s: string) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");

function selectedTests(cfg: RunConfiguration): DemoTest[] {
  const o = cfg.options ?? {};
  const scope = o.scope || "class";
  const pool = cfg.type === "kotest" ? TESTS.filter((t) => t.cls === SPEC) : TESTS.filter((t) => t.cls === JUNIT);
  switch (scope) {
    case "class":
      return pool.filter((t) => t.cls === o.class);
    case "test":
      return pool.filter((t) => t.cls === o.class && (t.name === o.test || t.name.startsWith(`${o.test} -- `)));
    case "method":
      return pool.filter((t) => t.cls === o.class && leaf(t.name) === o.method);
    case "pattern": {
      const wanted = (o.tests ?? "").split("\n").map((l) => l.trim().split("#")[0]);
      return pool.filter((t) => wanted.includes(t.cls));
    }
    default:
      return pool;
  }
}

function reports(tests: DemoTest[]): string[] {
  const byClass = new Map<string, DemoTest[]>();
  for (const t of tests) byClass.set(t.cls, [...(byClass.get(t.cls) ?? []), t]);
  return [...byClass.entries()].map(([cls, list]) => {
    const cases = list
      .map((t) => {
        const body = t.fail
          ? `<failure message="${esc(t.fail)}" type="io.kotest.assertions.AssertionFailedError">${esc(`io.kotest.assertions.AssertionFailedError: ${t.fail}\n\tat com.acme.orders.CartSpec$1$3$2.invokeSuspend(CartSpec.kt:21)`)}</failure>`
          : "";
        return `  <testcase name="${esc(t.name)}" classname="${cls}" time="${t.ms}">${body}</testcase>`;
      })
      .join("\n");
    return `<?xml version="1.0" encoding="UTF-8"?>\n<testsuite name="${cls}" tests="${list.length}" skipped="0" failures="${list.filter((t) => t.fail).length}" errors="0" time="0.2">\n${cases}\n</testsuite>`;
  });
}

const JACOCO = `<?xml version="1.0" encoding="UTF-8" standalone="yes"?><report name="orders-service"><package name="com/acme/orders">
<sourcefile name="Cart.kt"><line nr="12" mi="0" ci="6" mb="0" cb="2"/><line nr="13" mi="0" ci="4" mb="0" cb="0"/><line nr="14" mi="0" ci="2" mb="0" cb="0"/><line nr="18" mi="0" ci="8" mb="0" cb="0"/><line nr="19" mi="0" ci="7" mb="0" cb="0"/><line nr="22" mi="3" ci="0" mb="0" cb="0"/></sourcefile>
<sourcefile name="OrderService.kt"><line nr="7" mi="0" ci="3" mb="0" cb="0"/><line nr="8" mi="0" ci="9" mb="0" cb="0"/><line nr="9" mi="0" ci="4" mb="0" cb="0"/><line nr="10" mi="0" ci="6" mb="0" cb="2"/><line nr="11" mi="0" ci="4" mb="0" cb="0"/></sourcefile>
</package></report>`;

const gradleHeader = (tasks: string[]): Step[] => [[200, "Starting a Gradle Daemon (subsequent builds will be faster)\n"], [600, ""], ...tasks.map((t): Step => [90, `> Task ${t}\n`])];

function testSteps(cfg: RunConfiguration, debugPort: number | null, coverage: boolean): Step[] {
  const tests = selectedTests(cfg);
  const failed = tests.filter((t) => t.fail);
  const steps = gradleHeader([":checkKotlinGradlePluginConfigurationErrors", ":compileKotlin UP-TO-DATE", ":classes UP-TO-DATE", ":compileTestKotlin", ":testClasses", ":cleanTest", ":test"]);
  if (debugPort) steps.push([200, `Listening for transport dt_socket at address: ${debugPort}\n`], [4000, ""]);
  steps.push([500, ""]);
  for (const t of failed) steps.push([60, `\nCartSpec > ${t.name} FAILED\n    io.kotest.assertions.AssertionFailedError at CartSpec.kt:21\n`]);
  steps.push([120, `\n${tests.length} tests completed, ${failed.length} failed\n`]);
  if (coverage) steps.push([150, "\n> Task :jacocoTestReport\n"]);
  for (const xml of reports(tests)) steps.push([10, { report: xml }]);
  steps.push([80, failed.length ? "\nFAILURE: Build failed with an exception.\n\n* What went wrong:\nExecution failed for task ':test'.\n> There were failing tests.\n\nBUILD FAILED in 6s\n" : "\nBUILD SUCCESSFUL in 6s\n"], [20, { code: failed.length ? 1 : 0 }]);
  return steps;
}

function appSteps(cfg: RunConfiguration, debugPort: number | null): Step[] {
  const steps: Step[] = cfg.type === "kotlin-script" ? [] : gradleHeader([":compileKotlin", ":classes", ":forgeRunKotlin"]);
  if (debugPort) steps.push([150, `Listening for transport dt_socket at address: ${debugPort}\n`], [5000, ""]);
  if (cfg.type === "kotlin-script") steps.push([900, "Bumped orders-service to 2.2.0\n"], [60, { code: 0 }]);
  else for (let i = 1; i <= 5; i++) steps.push([250, `Order #${i}: 99.80 EUR\n`]);
  steps.push([120, { code: 0 }]);
  return steps;
}

const prevStart = mockHandler("run_start")!;
registerMock("run_start", async (args) => {
  const r = args.request as RunRequest;
  const cfg = r.command ? undefined : r.configuration;
  if (!cfg || !isKotlinRun(cfg)) return prevStart(args);
  const env = { ...(r.env ?? {}) };
  const debugPort = env[DEBUG_ENV] ? Number(env[DEBUG_ENV]) : null;
  const cmd = await kotlinCommand(cfg, !!r.coverage, debugPort);
  const line: CommandLine = { program: cmd.program, args: cmd.args, cwd: cfg.workingDirectory || "", env: cmd.env };
  const runId = r.runId ?? `mock-kotlin-run-${Date.now()}`;
  if (cfg.type === "kotest" || cfg.type === "junit" || cfg.type === "testng") {
    const test = cmd.test;
    const coverage: MockCoverage | undefined = r.coverage ? { name: cfg.name, text: () => JACOCO, rules: test?.coverage?.locationRules } : undefined;
    return startMockRun(runId, line, testSteps(cfg, debugPort, !!r.coverage), {
      testRunner: true,
      testReport: "jUnitFile",
      junitFlavor: "kotlin",
      configuration: cfg,
      locationRules: test?.locationRules,
      defaultTestFile: test?.defaultFile,
      coverage,
    });
  }
  return startMockRun(runId, line, appSteps(cfg, debugPort), { outputFilters: r.outputFilters });
});

const prevCommandLine = mockHandler("run_command_line")!;
registerMock("run_command_line", async (args) => {
  const r = args.request as RunRequest;
  const cfg = r.command ? undefined : r.configuration;
  if (!cfg || !isKotlinRun(cfg)) return prevCommandLine(args);
  const cmd = await kotlinCommand(cfg, !!r.coverage, null);
  return { program: cmd.program, args: cmd.args, cwd: cfg.workingDirectory || "", env: cmd.env };
});

// Run configurations of the demo, like a Kotlin developer's project.
if (isKotlinDemo()) {
  void Promise.resolve().then(async () => {
    const save = mockHandler("run_configurations_save");
    if (!save) return;
    await save({
      configurations: {
        selected: "MainKt",
        configurations: [
          { name: "MainKt", type: "java-application", storeAsProjectFile: true, options: { mainClass: "com.acme.orders.MainKt", kotlin: "true", file: `${KT_PKG}/Main.kt` } },
          { name: "orders-service [build]", type: "gradle", storeAsProjectFile: true, options: { tasks: "build" } },
          { name: "All specs", type: "kotest", storeAsProjectFile: true, options: { scope: "all", language: "kotlin", sourceRoot: "src/test/kotlin/" } },
          { name: "release.main.kts", type: "kotlin-script", options: { script: "scripts/release.main.kts" }, arguments: "2.2.0" },
        ],
      },
    });
  });
}

export { KT_TEST_PKG };
