/**
 * Browser mocks of DotNetForge's game engine commands (`unity_*`, `godot_*`) for the
 * `?project=unity` and `?project=godot` demos (`demoGameProjects.ts`). The Unity editor is
 * simulated: the bridge connects, play / pause / stop change its state and stream console
 * messages; tests, Godot runs and GdUnit4 print canned output with a canned report. Nothing
 * is written outside the in-memory demo files.
 */
import { emitMock, registerMock } from "@/ipc";
import type { BridgeEvent, GodotStatus, LogEntry, PlayMode, UnityStatus } from "@/ipc/gamedev";
import type { CommandLine, RunConfiguration, RunRequest } from "@/ipc/run";
import { mockSettingValue } from "./settingsMock";
import { mockFiles } from "./workspaceMock";
import { projectRoot, registerMockRunHandler, startMockRun, type Step } from "./runMock";
import {
  godotDemoFiles,
  godotDemoInfo,
  godotEditorClosed,
  isGodotDemo,
  isUnityDemo,
  unityDemoInfo,
} from "./demoGameProjects";

const UNITY = "/home/anna/Unity/Hub/Editor/2022.3.10f1/Editor/Unity";
const GODOT = "/home/anna/Applications/Godot_v4.3-stable_mono_linux_x86_64/Godot_v4.3-stable_mono_linux.x86_64";
const param = (key: string) => (typeof location !== "undefined" ? new URLSearchParams(location.search).get(key) : null);
/** `?project=unity&editor=closed`: Unity does not have the project open (tests can run). */
const unityClosed = () => param("editor") === "closed";

if (isUnityDemo() || isGodotDemo()) registerMock("project_detect", () => structuredClone(isUnityDemo() ? unityDemoInfo : godotDemoInfo));

// ---- Unity ----------------------------------------------------------------------------------

let connected = false;
let playMode: PlayMode = "edit";
const emit = (e: BridgeEvent) => emitMock("unity://bridge", e);
const log = (level: LogEntry["level"], message: string, stackTrace = "", location?: LogEntry["location"]) =>
  emit({ type: "log", entry: { level, message, stackTrace, time: new Date().toTimeString().slice(0, 8), location: location ?? null } });

const bridgeInstalled = () => !!mockFiles["Packages/dev.forge.unity/package.json"];

function unityStatus(): UnityStatus {
  const installed = bridgeInstalled();
  const running = !unityClosed();
  return {
    project: {
      version: "2022.3.10f1",
      revision: "ff3792e53c62",
      packages: [
        { name: "com.unity.ide.rider", version: "3.0.24" },
        { name: "com.unity.ide.visualstudio", version: "2.0.20" },
        { name: "com.unity.test-framework", version: "1.1.33" },
      ],
      idePackages: ["com.unity.ide.visualstudio", "com.unity.ide.rider"],
      testFramework: "1.1.33",
      bridge: installed ? { kind: "embedded", version: "1.0.0", update: false } : { kind: "none", update: false },
      solutions: ["Shooter.sln"],
      csprojects: ["Shooter.Game.csproj", "Shooter.Tests.EditMode.csproj"],
    },
    editors: [
      { version: "6000.0.23f1", path: "/home/anna/Unity/Hub/Editor/6000.0.23f1/Editor/Unity", source: "hub" },
      { version: "2022.3.10f1", path: UNITY, source: "hub" },
    ],
    editor: { version: "2022.3.10f1", path: UNITY, source: "hub" },
    exact: true,
    editorRunning: running ? 4242 : null,
    bridgeAvailable: running && installed,
    connected,
    welcome: connected ? { protocol: 1, unityVersion: "2022.3.10f1", projectPath: projectRoot(), pid: 4242, playMode, compiling: false } : null,
    bundledBridge: "1.0.0",
    remote: false,
  };
}

registerMock("unity_status", () => unityStatus());

registerMock("unity_install_bridge", () => {
  mockFiles["Packages/dev.forge.unity/package.json"] = `{\n  "name": "dev.forge.unity",\n  "version": "1.0.0",\n  "license": "MIT"\n}\n`;
  for (const f of ["Editor/Forge.Unity.Editor.asmdef", "Editor/ForgeBridge.cs", "Editor/ForgeCodeEditor.cs", "Editor/ForgeProjectFiles.cs", "LICENSE.md"])
    mockFiles[`Packages/dev.forge.unity/${f}`] = `// ${f} of the Forge editor bridge (dev.forge.unity 1.0.0, MIT)\n`;
  return { kind: "embedded", version: "1.0.0", update: false };
});

registerMock("unity_bridge_connect", () => {
  const s = unityStatus();
  if (!s.bridgeAvailable) throw new Error("The Forge editor bridge is not running: open the project in Unity with the bridge installed");
  connected = true;
  setTimeout(() => {
    log("log", "Forge editor bridge 1.0.0 listening on 127.0.0.1:52817", "Forge.Unity.Editor.ForgeBridge:.cctor () (at ./Packages/dev.forge.unity/Editor/ForgeBridge.cs:58)\n");
    log(
      "warning",
      "Assets/Scripts/Enemy.cs(13,22): warning CS0618: 'Object.FindObjectOfType<T>()' is obsolete: 'Object.FindObjectOfType has been deprecated. Use Object.FindFirstObjectByType instead.'",
      "",
      { path: "Assets/Scripts/Enemy.cs", line: 13, column: 22 },
    );
  }, 300);
  return unityStatus().welcome!;
});

registerMock("unity_bridge_disconnect", () => {
  connected = false;
  emit({ type: "disconnected", reason: "Closed by Forge" });
});

registerMock("unity_bridge_command", ({ command }) => {
  if (!connected) throw new Error("Not connected to the Unity editor");
  const state = (mode: PlayMode, compiling = false) => {
    playMode = mode;
    emit({ type: "state", playMode: mode, compiling });
  };
  switch (command) {
    case "play":
      state("playing");
      setTimeout(() => log("log", "Wave 1: 6 enemies", "Shooter.Spawner:StartWave (int) (at Assets/Scripts/Spawner.cs:24)\n"), 400);
      setTimeout(
        () =>
          log(
            "exception",
            "NullReferenceException: Object reference not set to an instance of an object\nShooter.Enemy.Update () (at Assets/Scripts/Enemy.cs:17)",
            "Shooter.Enemy.Update () (at Assets/Scripts/Enemy.cs:17)\n",
            { path: "Assets/Scripts/Enemy.cs", line: 17 },
          ),
        900,
      );
      setTimeout(() => log("log", "Player was destroyed", "UnityEngine.Debug:Log (object)\nShooter.PlayerController:TakeDamage (int) (at Assets/Scripts/PlayerController.cs:41)\n", { path: "Assets/Scripts/PlayerController.cs", line: 41 }), 1400);
      break;
    case "pause":
      state("paused");
      break;
    case "resume":
      state("playing");
      break;
    case "stop":
      state("edit");
      break;
    case "step":
      break;
    case "refresh":
      state(playMode, true);
      setTimeout(() => state(playMode, false), 900);
      break;
    case "syncSolution":
      return { ok: true, message: "Project files generated by Microsoft.Unity.VisualStudio.Editor" };
    default:
      return { ok: false, message: `unknown command ${command}` };
  }
  return { ok: true };
});

registerMock("unity_command", ({ action }) => {
  const root = projectRoot();
  if (action === "openEditor") {
    if (!unityClosed()) throw new Error("Unity already has the project open (process 4242)");
    return { program: UNITY, args: ["-projectPath", root], cwd: root } satisfies CommandLine;
  }
  if (!unityClosed()) throw new Error("Unity has the project open: connect to it (Tools | Unity | Connect to Unity Editor) to regenerate the project files there");
  return {
    program: UNITY,
    args: ["-batchmode", "-nographics", "-quit", "-projectPath", root, "-executeMethod", "Forge.Unity.Editor.ForgeProjectFiles.SyncFromCommandLine", "-logFile", "-"],
    cwd: root,
  } satisfies CommandLine;
});

registerMock("unity_processes", () =>
  unityClosed()
    ? []
    : [
        { pid: 4242, name: "Unity", project: projectRoot(), thisProject: true, debugPort: 56242 },
        { pid: 3107, name: "Unity", project: "/home/anna/Projects/Prototype", thisProject: false, debugPort: 56107 },
      ],
);

registerMock("unity_attach_request", ({ port, host, name }) => {
  if (!mockSettingValue("unity.debugger.adapterPath"))
    throw new Error(
      "Unity's debugger speaks Mono's soft debugger protocol, which Forge's debugger does not. Set a Mono debug adapter in Settings | Languages & Frameworks | .NET | Unity | Debugger to attach.",
    );
  return { adapter: "unity", mode: "attachPort", host: host ?? "127.0.0.1", port, name: name ?? `Unity (${host ?? "127.0.0.1"}:${port})` };
});

// ---- Godot ----------------------------------------------------------------------------------

registerMock("godot_status", (): GodotStatus => ({
  project: {
    name: "Dodge the Creeps",
    mainScene: "res://main.tscn",
    version: "4.3",
    features: ["4.3", "C#", "Forward Plus"],
    configVersion: 5,
    csharp: true,
    assemblyName: "Dodge the Creeps",
    autoloads: [{ name: "Score", path: "res://autoload/score.gd" }],
  },
  scenes: Object.keys(isGodotDemo() ? mockFiles : godotDemoFiles)
    .filter((p) => p.endsWith(".tscn"))
    .sort(),
  godot: { path: GODOT, source: "folder", dotnet: true },
  version: { version: "4.3", status: "stable", dotnet: true },
  host: "127.0.0.1",
  lspPort: 6005,
  dapPort: 6006,
  lspListening: !godotEditorClosed(),
  dapListening: !godotEditorClosed(),
  remote: false,
}));

registerMock("godot_command", () => ({ program: GODOT, args: ["--path", projectRoot(), "--editor"], cwd: projectRoot() }) satisfies CommandLine);

registerMock("godot_debug_request", ({ configuration }) => {
  const cfg = configuration as RunConfiguration;
  if (godotEditorClosed())
    throw new Error("The Godot editor's debug adapter does not answer on 127.0.0.1:6006: open the project in the Godot editor (Tools | Godot | Open in Godot Editor) and debug again");
  return {
    adapter: "godot",
    mode: "attachServer",
    host: "127.0.0.1",
    port: 6006,
    name: cfg.name,
    cwd: projectRoot(),
    configuration: { project: projectRoot(), scene: cfg.options?.target === "scene" ? `res://${cfg.options.scene}` : "main" },
  };
});

// ---- Runs -----------------------------------------------------------------------------------

const TYPES = new Set(["unity-test", "gdunit4", "godot", "gamedev-command"]);

function commandOf(cfg: RunConfiguration): CommandLine {
  const root = projectRoot();
  const o = cfg.options ?? {};
  switch (cfg.type) {
    case "unity-test": {
      const filter =
        o.scope === "class" ? `^${o.class?.replace(/\./g, "\\.")}\\.` : o.scope === "method" ? `^${o.class?.replace(/\./g, "\\.")}\\.${o.method}(\\(.*\\))?$` : o.scope === "filter" ? (o.filter ?? "") : "";
      return {
        program: UNITY,
        args: ["-batchmode", "-projectPath", root, "-runTests", "-testPlatform", o.platform || "EditMode", "-testResults", "/tmp/forge-test-reports/run/TestResults.xml", ...(filter ? ["-testFilter", filter] : []), "-logFile", "-"],
        cwd: root,
      };
    }
    case "gdunit4":
      return {
        program: GODOT,
        args: ["--headless", "--path", root, "-s", "-d", "res://addons/gdUnit4/bin/GdUnitCmdTool.gd", "--ignoreHeadlessMode", "-a", o.scope === "file" ? `res://${o.file}` : "res://test", "-rd", "/tmp/forge-test-reports/run"],
        cwd: root,
      };
    case "godot":
      return { program: GODOT, args: ["--path", root, ...(o.target === "scene" && o.scene ? [`res://${o.scene}`] : [])], cwd: root };
    default:
      return JSON.parse(o.commandLine ?? "{}") as CommandLine;
  }
}

const esc = (s: string) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");

function unityReport(cfg: RunConfiguration): string {
  const o = cfg.options ?? {};
  const C = "Shooter.Tests.PlayerTests";
  type Case = { name: string; method: string; result: string; ms: number; failure?: [string, string]; skip?: string };
  let cases: Case[] = [
    { name: "Jump_SetsVelocity", method: "Jump_SetsVelocity", result: "Passed", ms: 4 },
    { name: "Health_Decreases", method: "Health_Decreases", result: "Failed", ms: 12, failure: ["  Expected: 5\n  But was:  3\n", `at ${C}.Health_Decreases () [0x00010] in ${projectRoot()}/Assets/Tests/EditMode/PlayerTests.cs:29\n`] },
    { name: "Damage(10)", method: "Damage", result: "Passed", ms: 3 },
    { name: "Damage(-1)", method: "Damage", result: "Failed", ms: 7, failure: ["System.ArgumentOutOfRangeException : Damage must be positive\nParameter name: amount", `  at Shooter.PlayerController.TakeDamage (System.Int32 amount) [0x00012] in ${projectRoot()}/Assets/Scripts/PlayerController.cs:40\n  at ${C}.Damage (System.Int32 amount) [0x00001] in ${projectRoot()}/Assets/Tests/EditMode/PlayerTests.cs:36\n`] },
    { name: "Respawn", method: "Respawn", result: "Skipped", ms: 0, skip: "Waiting for the new spawn system" },
    { name: "FallsWithGravity", method: "FallsWithGravity", result: "Passed", ms: 21 },
  ];
  if (o.scope === "method") cases = cases.filter((c) => c.method === o.method);
  const body = cases
    .map((c) => {
      const inner = c.failure
        ? `<failure><message><![CDATA[${c.failure[0]}]]></message><stack-trace><![CDATA[${c.failure[1]}]]></stack-trace></failure>`
        : c.skip
          ? `<reason><message><![CDATA[${c.skip}]]></message></reason>`
          : "";
      return `<test-case name="${esc(c.name)}" fullname="${C}.${esc(c.name)}" methodname="${c.method}" classname="${C}" result="${c.result}" duration="${c.ms / 1000}">${inner}</test-case>`;
    })
    .join("\n");
  return `<?xml version="1.0" encoding="utf-8"?>\n<test-run id="2" testcasecount="${cases.length}" result="Failed">\n<test-suite type="Assembly" name="Shooter.Tests.EditMode.dll" fullname="${projectRoot()}/Library/ScriptAssemblies/Shooter.Tests.EditMode.dll">\n<test-suite type="TestFixture" name="PlayerTests" fullname="${C}" classname="${C}">\n${body}\n</test-suite>\n</test-suite>\n</test-run>\n`;
}

function gdunitReport(): string {
  return `<?xml version="1.0" encoding="UTF-8" ?>
<testsuites id="2026-11-04" name="report_1" tests="2" failures="1" skipped="0" flaky="0" time="0.214">
  <testsuite id="0" name="PlayerTest" package="test" timestamp="2026-11-04T10:02:11" hostname="localhost" tests="2" failures="1" errors="0" skipped="0" flaky="0" time="0.214">
    <testcase name="test_start_shows_the_player" classname="PlayerTest" time="0.105"></testcase>
    <testcase name="test_speed_default" classname="PlayerTest" time="0.109">
      <failure message="FAILED: res://test/player_test.gd:21" type="FAILURE"><![CDATA[Expecting:
 '450'
 but was
 '400']]></failure>
    </testcase>
  </testsuite>
</testsuites>
`;
}

function stepsOf(cfg: RunConfiguration): Step[] {
  switch (cfg.type) {
    case "unity-test": {
      if (!unityClosed())
        return [[60, "Unity has this project open: close the Unity editor to run its tests in batch mode (running tests inside the open editor is not supported yet)\n", "stderr"], [10, { code: 1 }]];
      return [
        [300, "[Licensing::Module] Successfully connected to LicensingClient\n"],
        [500, "Loading project: Shooter (2022.3.10f1)\n"],
        [900, "Refreshing native plugins compatible for Editor in 12.48 ms, found 3 plugins.\n"],
        [700, "Executing tests: EditMode\n"],
        [400, "Saving results to: /tmp/forge-test-reports/run/TestResults.xml\n"],
        [100, "Test run completed. Exiting with code 2 (Failed). One or more tests failed.\n"],
        [10, { report: unityReport(cfg) }],
        [10, { code: 2 }],
      ];
    }
    case "gdunit4":
      return [
        [200, "Godot Engine v4.3.stable.mono.official.77dcf97d8 - https://godotengine.org\n\n"],
        [300, "\x1b[36m--------------------------------------------------------------------------------------------------\x1b[0m\n"],
        [100, "GdUnit4 Comandline Tool\n"],
        [200, "Run Test Suite: res://test/player_test.gd\n"],
        [150, "  test_start_shows_the_player \x1b[32mPASSED\x1b[0m 105ms\n"],
        [150, "  test_speed_default \x1b[31mFAILED\x1b[0m 109ms\n    line 21: Expecting:\n      '450'\n     but was\n      '400'\n"],
        [100, "Overall Summary: 2 test cases | 0 errors | 1 failures | 0 flaky | 0 skipped | 0 orphans |\n"],
        [10, { report: gdunitReport() }],
        [10, { code: 100 }],
      ];
    case "godot":
      return [
        [200, "Godot Engine v4.3.stable.mono.official.77dcf97d8 - https://godotengine.org\n"],
        [100, "Vulkan 1.3.278 - Forward+ - Using Device #0: NVIDIA - NVIDIA GeForce RTX 3060\n\n"],
        [600, "Game started: score 0\n"],
        [900, "res://main.gd:23 - Mob spawned at (128, 0)\n"],
      ];
    default: {
      const cmd = commandOf(cfg);
      if (cmd.args.includes("--editor")) return [[200, "Godot Engine v4.3.stable.mono.official.77dcf97d8 - https://godotengine.org\n"], [400, "Editor started: GDScript language server on 6005, debug adapter on 6006\n"]];
      if (cmd.args.includes("-executeMethod")) return [[800, "Forge: Project files generated by Microsoft.Unity.VisualStudio.Editor\n"], [20, { code: 0 }]];
      return [[300, "Loading project: Shooter (2022.3.10f1)\n"]];
    }
  }
}

registerMockRunHandler({
  commandLine: async (r: RunRequest) => (r.configuration && TYPES.has(r.configuration.type) ? commandOf(r.configuration) : null),
  start: async (r: RunRequest, runId: string) => {
    const cfg = r.configuration;
    if (!cfg || !TYPES.has(cfg.type)) return null;
    const tests = cfg.type === "unity-test" || cfg.type === "gdunit4";
    return startMockRun(runId, commandOf(cfg), stepsOf(cfg), {
      testRunner: tests,
      testReport: tests ? "jUnitFile" : undefined,
      configuration: cfg,
      locationRules:
        cfg.type === "unity-test"
          ? [{ pattern: "^Shooter\\.Tests\\.PlayerTests$", replacement: "Assets/Tests/EditMode/PlayerTests.cs" }]
          : cfg.type === "gdunit4"
            ? [{ pattern: "^PlayerTest$", replacement: "test/player_test.gd" }]
            : [],
    });
  },
});
