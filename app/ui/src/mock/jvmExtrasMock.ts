/**
 * Browser-mode Groovy and Scala (JavaForge's JVM extras): the `groovy_*` and `scala_*`
 * commands and Groovy Script, sbt Task, ScalaTest and MUnit runs. Runnable scripts, main
 * objects, the sbt build (from the mock files and a recorded sbt import) and the command
 * lines go through the Rust crates (WebAssembly) like in the desktop app; the tools and the
 * run output are canned. `?project=scala` opens the sbt demo, `?project=android` the
 * Android demo with its Groovy release script. Nothing is installed or run.
 */
import { emitMock, mockHandler, registerMock } from "@/ipc";
import type { GroovyStatus } from "@/ipc/groovy";
import type { SbtBuild, ScalaStatus } from "@/ipc/scala";
import type { CommandLine, RunConfiguration, RunRequest } from "@/ipc/run";
import { callWasmSync, forgeWasm } from "@/wasm";
import { isScalaDemo, sbtImportOutput, scalaDemoInfo, scalaDemoName } from "./demoScalaProject";
import { mockFiles } from "./workspaceMock";
import { mockSettingValue } from "./settingsMock";
import { projectRoot, startMockRun, type Step } from "./runMock";

const HOME = "/home/anna";
const ROOT = () => projectRoot().replace(/^~/, HOME);
const METALS = "1.5.3";
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
const setting = (key: string) => String(mockSettingValue(key) ?? "").trim();

if (isScalaDemo()) {
  registerMock("project_detect", () => structuredClone(scalaDemoInfo));
}

// ---- Groovy ------------------------------------------------------------------------------------

const GROOVY = `${HOME}/.sdkman/candidates/groovy/current/bin/groovy`;
const gradleGroovy = () => ["build.gradle", "app/build.gradle"].some((f) => /id\s*\(?\s*['"]groovy['"]/.test(mockFiles[f] ?? ""));

registerMock("groovy_status", (): GroovyStatus => {
  const runner = setting("groovy.run.runner");
  const gradle = gradleGroovy();
  return {
    groovy: setting("groovy.sdk.path") || GROOVY,
    source: setting("groovy.sdk.path") ? "setting" : "SDKMAN!",
    version: "4.0.24",
    gradleGroovy: gradle,
    runner: runner === "gradle" || (runner !== "groovy" && gradle) ? "gradle" : "groovy",
  };
});

registerMock("groovy_runnable", async ({ text }) => callWasmSync((await forgeWasm()).groovy_runnable, { text }));

// ---- Scala: Metals and sbt -------------------------------------------------------------------

let metals: string | null =
  typeof location !== "undefined" && new URLSearchParams(location.search).get("metals") === "missing" ? null : METALS;

registerMock("scala_status", (): ScalaStatus => ({
  metals: metals ? `${HOME}/.local/share/forge/langservers/metals/${metals}/metals` : null,
  metalsVersion: metals,
  pinnedVersion: METALS,
  coursier: `${HOME}/.local/share/coursier/bin/cs`,
  sbt: `${HOME}/.sdkman/candidates/sbt/current/bin/sbt`,
  problems: metals ? [] : ["Metals is not installed"],
  remote: false,
}));

registerMock("scala_metals_install", async () => {
  const lines = [
    `Downloading https://repo1.maven.org/maven2/org/scalameta/metals_2.13/${METALS}/metals_2.13-${METALS}.pom`,
    `Downloaded https://repo1.maven.org/maven2/org/scalameta/metals_2.13/${METALS}/metals_2.13-${METALS}.jar`,
    "Downloading https://repo1.maven.org/maven2/ch/epfl/scala/bloop-launcher-core_2.13/2.0.6/bloop-launcher-core_2.13-2.0.6.jar",
    `Wrote ${HOME}/.local/share/forge/langservers/metals/${METALS}/metals`,
  ];
  for (const line of lines) {
    await sleep(250);
    emitMock("scala://install", { line, stderr: false });
  }
  metals = METALS;
  return METALS;
});

registerMock("scala_main_objects", async ({ text }) => callWasmSync((await forgeWasm()).scala_main_objects, { text }));

let imported = false;

async function sbtBuild(withImport: boolean): Promise<SbtBuild | null> {
  const build = mockFiles["build.sbt"];
  if (build === undefined) return null;
  const b = callWasmSync<SbtBuild>((await forgeWasm()).scala_sbt_build, {
    buildSbt: build,
    buildProperties: mockFiles["project/build.properties"] ?? null,
    pluginsSbt: mockFiles["project/plugins.sbt"] ?? null,
    folder: isScalaDemo() ? scalaDemoName : "project",
    syncOutput: withImport ? sbtImportOutput : null,
    root: ROOT(),
  });
  if (withImport) b.syncedAt = Math.floor(Date.now() / 1000) - 60;
  return b;
}

registerMock("scala_sbt_model", () => sbtBuild(imported));

registerMock("scala_sbt_import", async () => {
  const lines = sbtImportOutput.split("\n").filter(Boolean);
  emitMock("scala://sbt", { line: `${HOME}/.sdkman/candidates/sbt/current/bin/sbt -batch -no-colors projects "show scalaVersion" "show baseDirectory" "show libraryDependencies"`, stderr: false });
  for (const line of lines) {
    await sleep(60);
    emitMock("scala://sbt", { line, stderr: false });
  }
  imported = true;
  const b = await sbtBuild(true);
  if (!b) throw new Error("no build.sbt in the project root");
  return b;
});

registerMock("scala_sbt_cancel", () => undefined);

// ---- Runs ---------------------------------------------------------------------------------------

const SBT = `${HOME}/.sdkman/candidates/sbt/current/bin/sbt`;
const isExtrasRun = (cfg: RunConfiguration) => ["groovy-script", "sbt-task", "scalatest", "munit"].includes(cfg.type);

interface BuiltCommand {
  program: string;
  args: string[];
  env?: [string, string][];
  test?: { locationRules: { pattern: string; replacement: string }[]; defaultFile?: string };
}

async function commandOf(cfg: RunConfiguration): Promise<BuiltCommand> {
  const wasm = await forgeWasm();
  if (cfg.type === "groovy-script") {
    const runner = cfg.options?.runner || setting("groovy.run.runner") || "auto";
    const resolved = runner === "auto" ? (gradleGroovy() ? "gradle" : "groovy") : runner;
    const script = cfg.options?.script ?? "";
    return callWasmSync<BuiltCommand>(wasm.groovy_script_command, {
      run: {
        script: script.startsWith("/") ? script : `${ROOT()}/${script}`,
        arguments: cfg.arguments ?? "",
        vmOptions: cfg.interpreterOptions ?? "",
        workingDir: ROOT(),
        groovy: cfg.options?.groovy || GROOVY,
        buildProgram: `${ROOT()}/gradlew`,
        initScript: `${HOME}/.cache/forge/javaforge/forge-run-groovy.gradle`,
      },
      runner: resolved,
    });
  }
  return callWasmSync<BuiltCommand>(wasm.scala_command_line, { configuration: cfg, program: cfg.options?.sbt || SBT });
}

const sbtHeader: Step[] = [
  [150, "[info] welcome to sbt 1.10.5 (Eclipse Adoptium Java 21.0.5)\n"],
  [400, "[info] loading settings for project shop-build from plugins.sbt ...\n"],
  [300, "[info] loading project definition from /home/anna/Projects/shop/project\n"],
  [500, "[info] loading settings for project root from build.sbt ...\n"],
  [200, "[info] set current project to shop (in build file:/home/anna/Projects/shop/)\n"],
];

function groovySteps(cfg: RunConfiguration): Step[] {
  const version = (cfg.arguments ?? "").trim().split(/\s+/)[0] || "1.4.0";
  return [
    [700, `Bumped app/build.gradle to ${version}\n`],
    [80, `Release ${version}: offline sync, faster search\n`],
    [60, { code: 0 }],
  ];
}

function sbtSteps(cfg: RunConfiguration): Step[] {
  const tasks = (cfg.options?.tasks ?? "").split(/[\n;]/).map((t) => t.trim()).filter(Boolean);
  const steps = [...sbtHeader];
  if (!tasks.length) {
    steps.push([200, "[info] started sbt server\nsbt:shop> "]);
    return steps;
  }
  for (const t of tasks) {
    if (/runMain .*Main|run\b/.test(t)) {
      steps.push([600, "[info] compiling 1 Scala source to /home/anna/Projects/shop/modules/api/target/scala-2.13/classes ...\n"], [900, "[info] running com.acme.shop.api.Main\n"], [200, "Serving the shop API on http://localhost:8080 (1 demo item)\n"]);
    } else if (/report/.test(t)) {
      steps.push([600, "[info] running com.acme.shop.report\n"], [200, "2 items, 2.15 EUR\n"]);
    } else {
      steps.push([700, `[info] compiling 2 Scala sources to /home/anna/Projects/shop/core/target/scala-3.3.4/classes ...\n`]);
    }
  }
  steps.push([200, `[success] Total time: 3 s, completed Oct 27, 2026, 10:21:44 AM\n`], [40, { code: 0 }]);
  return steps;
}

const SPEC = "com.acme.shop.CartSpec";
const TESTS = [
  { cls: SPEC, name: "A Cart should add items", ms: 0.012 },
  { cls: SPEC, name: "A Cart should sum the total", ms: 0.004 },
  { cls: SPEC, name: "A Cart must reject negative quantities", ms: 0.006, fail: "IllegalArgumentException was not thrown" },
  { cls: "com.acme.shop.PriceSuite", name: "formats euros", ms: 0.003 },
  { cls: "com.acme.shop.PriceSuite", name: "rounds half up", ms: 0.002 },
];

function testSteps(cfg: RunConfiguration): Step[] {
  const o = cfg.options ?? {};
  const munit = cfg.type === "munit";
  const pool = TESTS.filter((t) => (munit ? t.cls.endsWith("Suite") : t.cls === SPEC));
  const selected = pool.filter((t) => (o.scope === "class" ? t.cls === o.class : o.scope === "test" ? t.name === o.test : true));
  const esc = (s: string) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/"/g, "&quot;");
  const byClass = new Map<string, typeof selected>();
  for (const t of selected) byClass.set(t.cls, [...(byClass.get(t.cls) ?? []), t]);
  const steps: Step[] = [...sbtHeader, [600, "[info] compiling 2 Scala sources to /home/anna/Projects/shop/core/target/scala-3.3.4/test-classes ...\n"]];
  for (const t of selected) steps.push([80, t.fail ? `[info] - ${t.name} *** FAILED ***\n[info]   ${t.fail} (CartSpec.scala:17)\n` : `[info] - ${t.name}\n`]);
  for (const [cls, list] of byClass) {
    const cases = list
      .map((t) => `  <testcase classname="${cls}" name="${esc(t.name)}" time="${t.ms}">${t.fail ? `<failure message="${esc(t.fail)}" type="org.scalatest.exceptions.TestFailedException">${esc(`${t.fail}\n\tat com.acme.shop.CartSpec.$anonfun$3(CartSpec.scala:17)`)}</failure>` : ""}</testcase>`)
      .join("\n");
    steps.push([10, { report: `<?xml version="1.0" encoding="UTF-8"?>\n<testsuite name="${cls}" tests="${list.length}" failures="${list.filter((t) => t.fail).length}" errors="0" time="0.1">\n${cases}\n</testsuite>` }]);
  }
  const failed = selected.filter((t) => t.fail).length;
  steps.push([100, failed ? `[error] Failed tests:\n[error] \t${SPEC}\n[error] (core / Test / testOnly) sbt.TestsFailedException: Tests unsuccessful\n` : "[info] All tests passed.\n"], [20, { code: failed ? 1 : 0 }]);
  return steps;
}

const prevStart = mockHandler("run_start")!;
registerMock("run_start", async (args) => {
  const r = args.request as RunRequest;
  const cfg = r.command ? undefined : r.configuration;
  if (!cfg || !isExtrasRun(cfg)) return prevStart(args);
  const cmd = await commandOf(cfg);
  const env = Object.fromEntries(cmd.env ?? []);
  const line: CommandLine = { program: cmd.program, args: cmd.args, cwd: cfg.workingDirectory || "", env };
  const runId = r.runId ?? `mock-jvm-run-${Date.now()}`;
  if (cfg.type === "scalatest" || cfg.type === "munit") {
    return startMockRun(runId, line, testSteps(cfg), {
      testRunner: true,
      testReport: "jUnitFile",
      junitFlavor: "kotlin",
      configuration: cfg,
      locationRules: cmd.test?.locationRules,
      defaultTestFile: cmd.test?.defaultFile,
    });
  }
  return startMockRun(runId, line, cfg.type === "groovy-script" ? groovySteps(cfg) : sbtSteps(cfg), { outputFilters: r.outputFilters });
});

const prevCommandLine = mockHandler("run_command_line")!;
registerMock("run_command_line", async (args) => {
  const r = args.request as RunRequest;
  const cfg = r.command ? undefined : r.configuration;
  if (!cfg || !isExtrasRun(cfg)) return prevCommandLine(args);
  const cmd = await commandOf(cfg);
  return { program: cmd.program, args: cmd.args, cwd: cfg.workingDirectory || "", env: Object.fromEntries(cmd.env ?? []) };
});

// Run configurations of the Scala demo.
if (isScalaDemo()) {
  void Promise.resolve().then(async () => {
    const save = mockHandler("run_configurations_save");
    if (!save) return;
    await save({
      configurations: {
        selected: "api/run",
        configurations: [
          { name: "api/run", type: "sbt-task", storeAsProjectFile: true, options: { tasks: "run", project: "api" } },
          { name: "~compile", type: "sbt-task", storeAsProjectFile: true, options: { tasks: "~compile" } },
          { name: "CartSpec", type: "scalatest", options: { scope: "class", class: SPEC, project: "core", projectDir: "core", file: "core/src/test/scala/com/acme/shop/CartSpec.scala" } },
        ],
      },
    });
  });
}
