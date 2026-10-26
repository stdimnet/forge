/**
 * Browser-mode JavaForge: the `java_*` commands and Java runs on the demo Gradle project
 * (`?project=java`, demoJavaProject.ts). Builds, main methods, test discovery and command
 * lines come from the Rust JavaForge crates through WebAssembly, like in the desktop app;
 * JDKs, JDT LS installs and the run output are canned. `?jdtls=missing` starts without JDT
 * LS so the install offer and its progress can be tried. Nothing is downloaded or run.
 */
import { emitMock, mockHandler, registerMock } from "@/ipc";
import type { AvailableReleases, BuildView, DebugPlan, InstallProgress, Jdk, JavaStatus } from "@/ipc/java";
import type { CommandLine, RunConfiguration, RunRequest } from "@/ipc/run";
import { callWasm, callWasmSync, forgeWasm } from "@/wasm";
import { isJavaDemo, JAVA_TEST_PKG, javaDemoInfo, javaDemoName } from "./demoJavaProject";
import { mockFiles } from "./workspaceMock";
import { mockSettingValue } from "./settingsMock";
import { projectRoot, startMockRun, type MockCoverage, type Step } from "./runMock";
import { markMockIntelephenseInstalled } from "./lspMock";

const HOME = "/home/anna";
const ROOT = () => projectRoot().replace(/^~/, HOME);

const JDKS: Jdk[] = [
  { home: `${HOME}/.local/share/forge/jdks/jdk-25+36`, version: "25", major: 25, vendor: "Eclipse Temurin", source: "Forge", name: "temurin-25" },
  { home: `${HOME}/.sdkman/candidates/java/21.0.4-tem`, version: "21.0.4", major: 21, vendor: "Eclipse Temurin", source: "SDKMAN!", name: "temurin-21" },
  { home: `${HOME}/.jdks/corretto-17.0.12`, version: "17.0.12", major: 17, vendor: "Amazon Corretto", source: "IntelliJ IDEA", name: "corretto-17" },
  { home: "/usr/lib/jvm/java-11-openjdk-amd64", version: "11.0.24", major: 11, vendor: "OpenJDK", source: "System", name: "openjdk-11" },
];
const jdks: Jdk[] = [...JDKS];

let jdtls: string | null =
  typeof location !== "undefined" && new URLSearchParams(location.search).get("jdtls") === "missing" ? null : "1.40.0-202409261450";

const setting = (key: string) => String(mockSettingValue(key) ?? "").trim();
const added = () => (mockSettingValue("java.jdks") as string[] | undefined) ?? [];

function allJdks(): Jdk[] {
  const extra = added()
    .filter((h) => !jdks.some((j) => j.home === h))
    .map((home): Jdk => ({ home, version: "22.0.2", major: 22, vendor: "Oracle OpenJDK", source: "Added", name: "openjdk-22" }));
  return [...jdks, ...extra].sort((a, b) => b.major - a.major);
}

const projectJdk = () => {
  const chosen = setting("java.projectJdk");
  const list = allJdks();
  return list.find((j) => j.home === chosen || j.name === chosen) ?? list.find((j) => j.major === 21) ?? list[0];
};

/** The demo build, read by `javaforge-build` (WebAssembly). */
async function buildView(): Promise<BuildView | null> {
  const wasm = await forgeWasm();
  return callWasmSync<BuildView | null>(wasm.java_build_info, buildRequest());
}

function buildRequest() {
  const files: Record<string, string> = {};
  for (const [path, text] of Object.entries(mockFiles)) if (!path.includes("/") || /(^|\/)(build\.gradle(\.kts)?|pom\.xml)$/.test(path)) files[path] = text;
  return {
    folder: javaDemoName,
    files,
    prefer: setting("java.build.preferredTool") || null,
    exec: { useWrapper: mockSettingValue("java.gradle.useWrapper") !== false, buildDir: ROOT(), windows: false },
  };
}

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

/** Canned install progress on `java://install`. */
async function simulateInstall(id: InstallProgress["id"], steps: string[], download: string, total: number): Promise<void> {
  const emit = (event: InstallProgress["event"]) => emitMock("java://install", { id, event } satisfies InstallProgress);
  for (const s of steps.slice(0, 1)) emit({ kind: "step", message: s });
  for (let done = 0; done <= total; done += total / 8) {
    await sleep(120);
    emit({ kind: "progress", label: download, done: Math.round(done), total });
  }
  for (const s of steps.slice(1)) {
    await sleep(150);
    emit({ kind: "step", message: s });
  }
}

if (isJavaDemo()) {
  registerMock("project_detect", () => structuredClone(javaDemoInfo));
}

registerMock("java_jdks", () => allJdks());
registerMock("java_jdk_inspect", ({ path }) => {
  const p = String(path ?? "");
  if (!/jdk|java|jvm/i.test(p)) return null;
  return { home: p, version: "22.0.2", major: 22, vendor: "Oracle OpenJDK", source: "Added", name: "openjdk-22" } satisfies Jdk;
});
registerMock("java_jdk_releases", (): AvailableReleases => ({
  available_lts_releases: [8, 11, 17, 21, 25],
  available_releases: [8, 11, 17, 21, 22, 23, 24, 25, 26],
  most_recent_lts: 25,
  most_recent_feature_release: 26,
}));
registerMock("java_jdk_download", async ({ feature }) => {
  const major = Number(feature);
  const release = `jdk-${major}.0.1+9`;
  await simulateInstall(
    "jdk",
    [`Looking up Eclipse Temurin ${major} for linux x64…`, `Verified SHA-256 of OpenJDK${major}U-jdk_x64_linux_hotspot.tar.gz.`, `Installed ${release}.`],
    `Downloading ${release}`,
    196_000_000,
  );
  const jdk: Jdk = { home: `${HOME}/.local/share/forge/jdks/${release}`, version: `${major}.0.1`, major, vendor: "Eclipse Temurin", source: "Forge", name: `temurin-${major}` };
  if (!jdks.some((j) => j.home === jdk.home)) jdks.push(jdk);
  return jdk;
});

registerMock("java_status", async (): Promise<JavaStatus> => {
  const build = await buildView();
  const server = allJdks().find((j) => j.major >= 21) ?? null;
  return {
    jdtls,
    jdtlsDir: jdtls ? `${HOME}/.local/share/forge/langservers/jdtls/${jdtls}` : null,
    javaDebug: jdtls ? `${HOME}/.local/share/forge/langservers/java-debug/com.microsoft.java.debug.plugin-0.53.1.jar` : null,
    serverJdk: server,
    projectJdk: projectJdk(),
    languageLevel: setting("java.languageLevel") || build?.project.javaVersion || String(projectJdk()?.major ?? ""),
    build: build?.project ?? null,
    problems: jdtls ? [] : ["Eclipse JDT Language Server is not installed"],
    remote: false,
  };
});

registerMock("java_jdtls_install", async () => {
  await simulateInstall(
    "jdtls",
    [
      "Looking up the newest Eclipse JDT LS on https://download.eclipse.org/jdtls…",
      "Verified SHA-256 of jdt-language-server-1.40.0-202409261450.tar.gz.",
      "Verified SHA-1 of com.microsoft.java.debug.plugin-0.53.1.jar.",
    ],
    "Downloading JDT LS 1.40.0-202409261450",
    52_000_000,
  );
  jdtls = "1.40.0-202409261450";
  // The mock language server (here: JDT LS) starts for the open files.
  markMockIntelephenseInstalled();
  return jdtls;
});

registerMock("java_build_info", () => buildView());
registerMock("java_reload_project", () => undefined);
registerMock("java_main_methods", async ({ path, text }) => callWasm((await forgeWasm()).java_main_methods, { path, text }));
registerMock("java_debug_prepare", ({ configuration }): DebugPlan => {
  const cfg = configuration as RunConfiguration;
  if (!jdtls) throw new Error("The Java debugger (java-debug) is not installed: run Install Eclipse JDT Language Server");
  if (cfg.type === "jvm-remote-debug") {
    return { dapPort: 41321, hostName: cfg.options?.host || "localhost", jdwpPort: Number(cfg.options?.port || 5005), run: false };
  }
  const gradle = cfg.type === "gradle" || cfg.type === "junit" || cfg.type === "testng";
  return { dapPort: 41321, hostName: "127.0.0.1", jdwpPort: gradle ? 5005 : 47011, run: true };
});

// ---- Runs ---------------------------------------------------------------------------------

const JAVA_TYPES = new Set(["java-application", "gradle", "maven", "junit", "testng", "jvm-remote-debug"]);
const DEBUG_ENV = "FORGE_JAVA_DEBUG_PORT";

interface JavaCommand {
  program: string;
  args: string[];
  test?: { locationRules: { pattern: string; replacement: string }[]; defaultFile?: string; coverage?: { file: string; locationRules?: { pattern: string; replacement: string }[] } };
}

async function javaCommand(cfg: RunConfiguration, coverage: boolean, debugPort: number | null): Promise<JavaCommand> {
  const wasm = await forgeWasm();
  const java = `${(cfg.options?.jdk || projectJdk()?.home) ?? "/usr"}/bin/java`;
  return callWasmSync<JavaCommand>(wasm.java_command_line, { configuration: cfg, build: buildRequest(), coverage, java, debugPort });
}

interface DemoTest {
  cls: string;
  file: string;
  name: string;
  ms: number;
  fail?: string;
}

const TESTS: DemoTest[] = [
  { cls: "com.acme.inventory.InventoryTest", file: `${JAVA_TEST_PKG}/InventoryTest.java`, name: "addsStockOfTheSameSku()", ms: 0.012 },
  { cls: "com.acme.inventory.InventoryTest", file: `${JAVA_TEST_PKG}/InventoryTest.java`, name: "removesStock()", ms: 0.009, fail: "expected: <4> but was: <3>" },
  { cls: "com.acme.inventory.InventoryTest", file: `${JAVA_TEST_PKG}/InventoryTest.java`, name: "refusesToRemoveMoreThanInStock(int)[1]", ms: 0.004 },
  { cls: "com.acme.inventory.InventoryTest", file: `${JAVA_TEST_PKG}/InventoryTest.java`, name: "refusesToRemoveMoreThanInStock(int)[2]", ms: 0.002 },
  { cls: "com.acme.inventory.InventoryTest$LowStock", file: `${JAVA_TEST_PKG}/InventoryTest.java`, name: "listsItemsBelowTheThreshold()", ms: 0.003 },
  { cls: "com.acme.inventory.StockItemTest", file: `${JAVA_TEST_PKG}/StockItemTest.java`, name: "rejectsNegativeQuantities()", ms: 0.006 },
];

const method = (name: string) => name.replace(/[([].*$/, "");
const esc = (s: string) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");

function selectedTests(cfg: RunConfiguration): DemoTest[] {
  const o = cfg.options ?? {};
  switch (o.scope || "class") {
    case "class":
      return TESTS.filter((t) => t.cls === o.class || t.cls.startsWith(`${o.class}$`));
    case "method":
      return TESTS.filter((t) => (t.cls === o.class || t.cls.startsWith(`${o.class}$`)) && method(t.name) === o.method);
    case "package":
      return TESTS.filter((t) => t.cls.startsWith(`${o.package}.`));
    case "pattern": {
      const wanted = (o.tests ?? "").split("\n").map((l) => l.trim()).filter(Boolean);
      return TESTS.filter((t) => wanted.some((w) => w === t.cls || w === `${t.cls}#${method(t.name)}`));
    }
    default:
      return TESTS;
  }
}

/** Gradle's `TEST-<class>.xml` of the selected tests. */
function reports(tests: DemoTest[]): string[] {
  const byClass = new Map<string, DemoTest[]>();
  for (const t of tests) byClass.set(t.cls, [...(byClass.get(t.cls) ?? []), t]);
  return [...byClass.entries()].map(([cls, list]) => {
    const failures = list.filter((t) => t.fail).length;
    const cases = list
      .map((t) => {
        const body = t.fail
          ? `<failure message="${esc(t.fail)}" type="org.opentest4j.AssertionFailedError">${esc(`org.opentest4j.AssertionFailedError: ${t.fail}\n\tat org.junit.jupiter.api.AssertionFailureBuilder.build(AssertionFailureBuilder.java:151)\n\tat org.junit.jupiter.api.Assertions.assertEquals(Assertions.java:531)\n\tat com.acme.inventory.InventoryTest.removesStock(InventoryTest.java:31)`)}</failure>`
          : "";
        return `  <testcase name="${esc(t.name)}" classname="${cls}" time="${t.ms}">${body}</testcase>`;
      })
      .join("\n");
    return `<?xml version="1.0" encoding="UTF-8"?>\n<testsuite name="${cls}" tests="${list.length}" skipped="0" failures="${failures}" errors="0" time="0.05">\n${cases}\n</testsuite>`;
  });
}

const JACOCO = `<?xml version="1.0" encoding="UTF-8" standalone="yes"?><!DOCTYPE report PUBLIC "-//JACOCO//DTD Report 1.1//EN" "report.dtd"><report name="inventory-service"><package name="com/acme/inventory">
<sourcefile name="Inventory.java"><line nr="11" mi="0" ci="4" mb="0" cb="0"/><line nr="14" mi="0" ci="6" mb="0" cb="0"/><line nr="18" mi="0" ci="5" mb="0" cb="0"/><line nr="19" mi="0" ci="2" mb="1" cb="1"/><line nr="20" mi="6" ci="0" mb="0" cb="0"/><line nr="22" mi="0" ci="5" mb="0" cb="0"/><line nr="23" mi="0" ci="2" mb="0" cb="2"/><line nr="24" mi="0" ci="9" mb="0" cb="0"/><line nr="26" mi="0" ci="6" mb="0" cb="0"/><line nr="30" mi="0" ci="4" mb="0" cb="0"/><line nr="31" mi="0" ci="10" mb="0" cb="2"/><line nr="32" mi="0" ci="4" mb="0" cb="2"/><line nr="33" mi="0" ci="4" mb="0" cb="0"/><line nr="36" mi="0" ci="2" mb="0" cb="0"/><line nr="40" mi="0" ci="4" mb="0" cb="0"/></sourcefile>
<sourcefile name="StockItem.java"><line nr="6" mi="0" ci="2" mb="0" cb="2"/><line nr="7" mi="0" ci="5" mb="0" cb="0"/><line nr="12" mi="0" ci="7" mb="0" cb="0"/><line nr="16" mi="0" ci="6" mb="0" cb="0"/></sourcefile>
<sourcefile name="App.java"><line nr="9" mi="4" ci="0" mb="0" cb="0"/><line nr="10" mi="9" ci="0" mb="0" cb="0"/><line nr="11" mi="9" ci="0" mb="0" cb="0"/><line nr="12" mi="4" ci="0" mb="0" cb="0"/><line nr="14" mi="4" ci="0" mb="0" cb="0"/><line nr="15" mi="14" ci="0" mb="0" cb="0"/><line nr="16" mi="10" ci="0" mb="2" cb="0"/><line nr="17" mi="12" ci="0" mb="0" cb="0"/></sourcefile>
</package></report>`;

const gradleHeader = (tasks: string[]): Step[] => [
  [200, "Starting a Gradle Daemon (subsequent builds will be faster)\n"],
  [600, ""],
  ...tasks.map((t): Step => [90, `> Task ${t}\n`]),
];

function testSteps(cfg: RunConfiguration, debugPort: number | null, coverage: boolean): Step[] {
  const tests = selectedTests(cfg);
  const failed = tests.some((t) => t.fail);
  const steps: Step[] = gradleHeader([":compileJava UP-TO-DATE", ":processResources NO-SOURCE", ":classes UP-TO-DATE", ":compileTestJava", ":processTestResources NO-SOURCE", ":testClasses", ":cleanTest", ":test"]);
  if (debugPort) steps.push([200, `Listening for transport dt_socket at address: ${debugPort}\n`], [4000, ""]);
  steps.push([400, ""]);
  for (const t of tests.filter((t) => t.fail)) {
    steps.push([60, `\nInventoryTest > ${t.name} FAILED\n    org.opentest4j.AssertionFailedError at InventoryTest.java:31\n`]);
  }
  steps.push([120, `\n${tests.length} tests completed, ${tests.filter((t) => t.fail).length} failed\n`]);
  if (coverage) steps.push([150, "\n> Task :jacocoTestReport\n"]);
  for (const xml of reports(tests)) steps.push([10, { report: xml }]);
  steps.push(
    [80, failed ? "\nFAILURE: Build failed with an exception.\n\n* What went wrong:\nExecution failed for task ':test'.\n> There were failing tests. See the report at: file://" + ROOT() + "/build/reports/tests/test/index.html\n\nBUILD FAILED in 4s\n" : "\nBUILD SUCCESSFUL in 4s\n"],
    [20, { code: failed ? 1 : 0 }],
  );
  return steps;
}

function appSteps(debugPort: number | null): Step[] {
  const steps: Step[] = [];
  if (debugPort) steps.push([150, `Listening for transport dt_socket at address: ${debugPort}\n`], [5000, ""]);
  steps.push([300, "2 items, 1 low on stock\n"], [40, "Reorder Anvil (4 left)\n"], [120, { code: 0 }]);
  return steps;
}

function gradleSteps(cfg: RunConfiguration, debugPort: number | null): Step[] {
  const tasks = (cfg.options?.tasks ?? "").split(/\s+/).filter(Boolean);
  const run = tasks.includes("run");
  const steps = gradleHeader([":compileJava", ":processResources NO-SOURCE", ":classes", ...tasks.filter((t) => !["classes", "compileJava"].includes(t)).map((t) => `:${t}`)]);
  if (run && debugPort) steps.push([150, `Listening for transport dt_socket at address: ${debugPort}\n`], [5000, ""]);
  if (run) steps.push([300, "2 items, 1 low on stock\n"], [40, "Reorder Anvil (4 left)\n"]);
  steps.push([200, `\nBUILD SUCCESSFUL in 3s\n${tasks.length + 2} actionable tasks: ${tasks.length + 1} executed, 1 up-to-date\n`], [20, { code: 0 }]);
  return steps;
}

const prevStart = mockHandler("run_start")!;
registerMock("run_start", async (args) => {
  const r = args.request as RunRequest;
  const cfg = r.command ? undefined : r.configuration;
  if (!cfg || !JAVA_TYPES.has(cfg.type)) return prevStart(args);
  const env = { ...(r.env ?? {}) };
  const debugPort = env[DEBUG_ENV] ? Number(env[DEBUG_ENV]) : null;
  if (cfg.type === "jvm-remote-debug") throw new Error("Remote JVM Debug configurations attach the debugger; start them with Debug");
  const cmd = await javaCommand(cfg, !!r.coverage, debugPort);
  const line: CommandLine = { program: cmd.program, args: cmd.args, cwd: cfg.workingDirectory || "" };
  const runId = r.runId ?? `mock-java-run-${Date.now()}`;
  const test = cmd.test;
  if (cfg.type === "junit" || cfg.type === "testng") {
    let coverage: MockCoverage | undefined;
    if (r.coverage) coverage = { name: cfg.name, text: () => JACOCO, rules: test?.coverage?.locationRules };
    return startMockRun(runId, line, testSteps(cfg, debugPort, !!r.coverage), {
      testRunner: true,
      testReport: "jUnitFile",
      junitFlavor: "java",
      configuration: cfg,
      locationRules: test?.locationRules,
      defaultTestFile: test?.defaultFile,
      coverage,
    });
  }
  return startMockRun(runId, line, cfg.type === "java-application" ? appSteps(debugPort) : gradleSteps(cfg, debugPort), { outputFilters: r.outputFilters });
});

const prevCommandLine = mockHandler("run_command_line")!;
registerMock("run_command_line", async (args) => {
  const r = args.request as RunRequest;
  const cfg = r.command ? undefined : r.configuration;
  if (!cfg || !JAVA_TYPES.has(cfg.type)) return prevCommandLine(args);
  const cmd = await javaCommand(cfg, !!r.coverage, null);
  return { program: cmd.program, args: cmd.args, cwd: cfg.workingDirectory || "" };
});

// Run configurations of the demo, like a Java developer's project.
if (isJavaDemo()) {
  void Promise.resolve().then(async () => {
    const save = mockHandler("run_configurations_save");
    if (!save) return;
    await save({
      configurations: {
        selected: "App",
        configurations: [
          { name: "App", type: "java-application", storeAsProjectFile: true, options: { mainClass: "com.acme.inventory.App" } },
          { name: "inventory-service [build]", type: "gradle", storeAsProjectFile: true, options: { tasks: "build" } },
          { name: "All in inventory-service", type: "junit", storeAsProjectFile: true, options: { scope: "all" } },
          { name: "Attach to staging", type: "jvm-remote-debug", options: { host: "staging.acme.local", port: "5005" } },
        ],
      },
    });
  });
}

