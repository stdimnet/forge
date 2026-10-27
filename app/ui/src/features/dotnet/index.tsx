/**
 * DotNetForge in the UI (Rider's .NET support). Everything is computed in Rust
 * (`src/ipc/dotnet.ts`, crates `packs/dotnetforge/*`); this module plugs it into the
 * contribution points:
 *
 * - Settings | Languages & Frameworks | .NET (host, SDKs, global.json, Roslyn, netcoredbg,
 *   CLI, build, Code Cleanup, NuGet),
 * - the Build tool window (solution tree, build problems and output) and the NuGet tool
 *   window (search, installed packages, upgrades),
 * - the ".NET Project" run configuration type (dotnet run / dotnet watch, launch
 *   profiles), ".NET Tests" (xUnit, NUnit, MSTest through dotnet test and TRX), Debug with
 *   netcoredbg (`dotnet_debug_request`; tests attach to the waiting test host),
 * - Build / Rebuild / Clean Solution (Ctrl+F9 builds the solution), Restore, Reload
 *   Solution, Add Project from Template…, Manage NuGet Packages, Install .NET SDK…, the
 *   Roslyn and netcoredbg downloads,
 * - the editor (`editor.ts`): MSBuild, Razor and `.sln` highlighting, Reformat Code with
 *   dotnet format, Code Cleanup with dotnet format.
 */
import { createEffect, createRoot, on } from "solid-js";
import { AppWindow, FlaskConical, Hammer, Package, Terminal } from "lucide-solid";
import { dotnetApi } from "@/ipc/dotnet";
import { listen } from "@/ipc";
import { runApi, type RunConfiguration, type RunEvent } from "@/ipc/run";
import { registerKeymapActions } from "@/keymap";
import { registerSettingsPage } from "@/features/settingsPages";
import { registerToolWindow } from "@/features/registry";
import { action, registerMainMenuEntries } from "@/features/mainMenu/model";
import { refreshDebugAdapters } from "@/features/debug/adapters";
import { dapApi } from "@/features/debug/dapApi";
import { registerDebugLauncher, registerRunConfigurationType } from "@/features/run/types";
import { isDialogWindow } from "@/shell/window/mode";
import { commands, registerCommand } from "@/state/commands";
import { showToolWindow } from "@/state/layout";
import { getSetting } from "@/state/settings";
import { workspace } from "@/state/workspace";
import { safeMode } from "@/features/trust/store";
import { BuildToolWindow } from "./BuildToolWindow";
import { NuGetToolWindow, refreshUpdates } from "./NuGetToolWindow";
import { DotnetSettingsPage } from "./DotnetSettingsPage";
import { openAddProjectDialog } from "./AddProjectDialog";
import { installNetcoredbg, installRoslyn, installSdk } from "./install";
import "./editor";
import {
  dotnetActive,
  errorText,
  PLUGIN,
  projectByPath,
  publishBuildOutput,
  refreshDotnetStatus,
  refreshSolution,
  runBuild,
  runnableProjects,
  setDotnetBuilding,
  showDotnetBalloon,
  testProjects,
} from "./state";

const opt = (cfg: RunConfiguration, key: string) => cfg.options?.[key] ?? "";

// ---- Settings and tool windows -----------------------------------------------------------------

registerSettingsPage({
  id: "lang.dotnet",
  title: ".NET",
  group: "Languages & Frameworks",
  order: 28,
  keywords: [".net", "dotnet", "c#", "csharp", "f#", "fsharp", "sdk", "global.json", "msbuild", "roslyn", "netcoredbg", "nuget", "dotnet format", "editorconfig", "code cleanup", "solution"],
  component: DotnetSettingsPage,
});

registerToolWindow({
  id: "dotnet-build",
  title: "Build",
  icon: Hammer,
  anchor: "bottom",
  order: 46,
  component: BuildToolWindow,
  when: () => dotnetActive(),
});

registerToolWindow({
  id: "nuget",
  title: "NuGet",
  icon: Package,
  anchor: "bottom",
  order: 47,
  component: NuGetToolWindow,
  when: () => dotnetActive(),
});

// ---- Run configurations -------------------------------------------------------------------------

const projectOptions = () => runnableProjects().map((p) => ({ value: p.path, label: p.name }));
/** The launch profiles `dotnet run` can start, of every runnable project. */
const profileOptions = () => {
  const out = new Map<string, string>();
  for (const p of runnableProjects())
    for (const x of p.launchProfiles ?? [])
      if ((x.commandName === "Project" || x.commandName === "Executable") && !out.has(x.name)) out.set(x.name, `${x.name}${x.applicationUrl ? `  ${x.applicationUrl.split(";")[0]}` : ""}`);
  return [{ value: "", label: "Default (the first profile)" }, ...[...out].map(([value, label]) => ({ value, label })), { value: "none", label: "None (--no-launch-profile)" }];
};

/** Build the solution first (`dotnet.build.beforeRun`), so its problems reach the Build tool window. */
async function buildFirst(): Promise<boolean> {
  if (!getSetting<boolean>("dotnet.build.beforeRun", false)) return true;
  return (await runBuild("build")) === 0;
}

registerRunConfigurationType({
  id: "dotnet",
  name: ".NET Project",
  description: "Run a .NET project with dotnet run or dotnet watch, optionally with a launch profile of Properties/launchSettings.json; Debug starts the built assembly under netcoredbg.",
  icon: AppWindow,
  color: "var(--syn-function)",
  pluginId: PLUGIN,
  argumentsLabel: "Program arguments",
  debuggable: true,
  noRunTarget: true,
  defaults: () => {
    const p = runnableProjects()[0];
    const profile = p?.launchProfiles?.find((x) => x.commandName === "Project");
    return { options: { project: p?.path ?? "", ...(profile ? { profile: profile.name } : {}), mode: "run" } };
  },
  fields: [
    { key: "project", label: "Project", kind: "enum", required: true, options: projectOptions },
    {
      key: "profile",
      label: "Launch profile",
      kind: "enum",
      default: "",
      options: profileOptions,
    },
    {
      key: "mode",
      label: "Run with",
      kind: "segmented",
      default: "run",
      options: [
        { value: "run", label: "dotnet run" },
        { value: "watch", label: "dotnet watch (hot reload)" },
      ],
    },
    { key: "framework", label: "Target framework", placeholder: "net8.0 (the project's first when empty)" },
    {
      key: "configuration",
      label: "Configuration",
      kind: "enum",
      default: "",
      options: [
        { value: "", label: "Settings | .NET" },
        { value: "Debug", label: "Debug" },
        { value: "Release", label: "Release" },
      ],
    },
    { key: "noBuild", label: "Do not build (--no-build)", kind: "bool" },
    { key: "dotnetArgs", label: "dotnet run options", placeholder: "--property:DefineConstants=TRACE" },
  ],
  suggestName: (cfg) => {
    const p = projectByPath(opt(cfg, "project"));
    const name = p?.name ?? opt(cfg, "project").split("/").pop()?.replace(/\.\w+proj$/, "");
    const profile = opt(cfg, "profile");
    return name ? (profile && profile !== "none" ? `${name}: ${profile}` : name) : undefined;
  },
  start: async (_cfg, request) => {
    await buildFirst();
    return runApi.start(request);
  },
});

const scope = (cfg: RunConfiguration) => opt(cfg, "scope") || "all";
const inScope = (...list: string[]) => (cfg: RunConfiguration) => list.includes(scope(cfg));

registerRunConfigurationType({
  id: "dotnet-test",
  name: ".NET Tests",
  description: "Run xUnit, NUnit or MSTest tests with dotnet test: the solution, a project, a class, a method or a filter; results from the TRX report.",
  icon: FlaskConical,
  color: "var(--success)",
  pluginId: PLUGIN,
  argumentsLabel: "RunSettings arguments",
  debuggable: true,
  testRunner: true,
  fields: [
    {
      key: "scope",
      label: "Test scope",
      kind: "segmented",
      default: "all",
      options: [
        { value: "all", label: "Solution" },
        { value: "project", label: "Project" },
        { value: "class", label: "Class" },
        { value: "method", label: "Method" },
        { value: "filter", label: "Filter" },
      ],
    },
    {
      key: "project",
      label: "Test project",
      kind: "enum",
      required: true,
      options: () => testProjects().map((p) => ({ value: p.path, label: p.name })),
      visible: inScope("project", "class", "method", "filter"),
    },
    { key: "class", label: "Class", required: true, placeholder: "Bookstore.Tests.CartTests", visible: inScope("class", "method") },
    { key: "method", label: "Method", required: true, placeholder: "Total_AddsLines", visible: inScope("method") },
    { key: "filter", label: "Filter", required: true, placeholder: "FullyQualifiedName~Cart&Category!=Slow", visible: inScope("filter") },
    { key: "framework", label: "Target framework", placeholder: "net8.0" },
    { key: "noBuild", label: "Do not build (--no-build)", kind: "bool" },
    { key: "settings", label: "RunSettings file", kind: "path", placeholder: "tests.runsettings" },
    { key: "flags", label: "dotnet test options", placeholder: "--blame-hang-timeout 60s" },
  ],
  suggestName: (cfg) => {
    switch (scope(cfg)) {
      case "project":
        return projectByPath(opt(cfg, "project"))?.name;
      case "class":
        return opt(cfg, "class").split(/[.+]/).pop() || undefined;
      case "method":
        return opt(cfg, "method") ? `${opt(cfg, "class").split(/[.+]/).pop()}.${opt(cfg, "method")}` : undefined;
      case "filter":
        return opt(cfg, "filter") || undefined;
      default:
        return "All tests";
    }
  },
  start: async (_cfg, request) => {
    await buildFirst();
    return runApi.start(request);
  },
});

let commandSeq = 0;
/** Build, NuGet, template and cleanup command lines; builds' output becomes the Build tool window's problems. */
registerRunConfigurationType({
  id: "dotnet-command",
  name: ".NET",
  icon: Terminal,
  hidden: true,
  fields: [],
  argumentsLabel: false,
  buildCommand: (cfg) => JSON.parse(cfg.options?.commandLine ?? "{}"),
  start: async (cfg, request) => {
    if (cfg.options?.build !== "true") return runApi.start(request);
    const runId = request.runId ?? `dotnet-build-${++commandSeq}`;
    const chunks: string[] = [];
    const off = await runApi.onEvent(runId, (e: RunEvent) => {
      if (e.type === "output") chunks.push(e.text);
      if (e.type === "exited") {
        off();
        setDotnetBuilding(null);
        void publishBuildOutput(chunks.join("")).then((r) => {
          if (r?.succeeded === false) showToolWindow("dotnet-build");
        });
      }
    });
    return runApi.start({ ...request, runId });
  },
});

// ---- Debugging with netcoredbg ------------------------------------------------------------------

/** VSTest's test host waits for a debugger and prints its process id. */
const TEST_HOST = /Process Id:\s*(\d+),\s*Name:\s*testhost/i;

registerDebugLauncher({
  id: "dotnet",
  canDebug: (cfg) => (cfg.type === "dotnet" || cfg.type === "dotnet-test") && dotnetActive(),
  async launch(cfg, ctx) {
    try {
      await refreshDebugAdapters();
      if (cfg.type === "dotnet") {
        showDotnetBalloon(`Building '${cfg.name}' for debugging…`, "", 3000);
        const request = await dotnetApi.debugRequest(cfg);
        showToolWindow("debug");
        await dapApi.start(request);
        return;
      }
      // Tests: dotnet test with VSTEST_HOST_DEBUG, then attach to the waiting test host.
      const handle = await ctx.run({ env: { VSTEST_HOST_DEBUG: "1" } });
      if (!handle) return;
      let attached = false;
      const unlisten = await listen<RunEvent>(`run://${handle.started.runId}`, (e) => {
        const m = e.type === "output" ? TEST_HOST.exec(e.text) : null;
        if (!m || attached) return;
        attached = true;
        void dotnetApi
          .attachRequest(Number(m[1]), cfg.name)
          .then((request) => {
            showToolWindow("debug");
            return dapApi.start(request);
          })
          .catch((err) => showDotnetBalloon(`Cannot attach to the test host of '${cfg.name}'`, errorText(err), 8000));
      });
      void handle.exited.then(() => unlisten());
    } catch (e) {
      showDotnetBalloon(`Cannot debug '${cfg.name}'`, errorText(e), 10000);
      if (/netcoredbg/.test(errorText(e))) void installNetcoredbg();
    }
  },
});

// ---- Actions --------------------------------------------------------------------------------------

const needsDotnet = (run: () => void) => () => {
  if (!dotnetActive()) {
    showDotnetBalloon("No .NET solution", "Open a folder with a .sln, .slnx or project file, or enable the .NET plugin.", 5000);
    return;
  }
  run();
};

registerKeymapActions([
  { id: "dotnet.buildSolution", title: "Build Solution", group: "Plugins/.NET" },
  { id: "dotnet.rebuildSolution", title: "Rebuild Solution", group: "Plugins/.NET" },
  { id: "dotnet.cleanSolution", title: "Clean Solution", group: "Plugins/.NET" },
  { id: "dotnet.restore", title: "Restore NuGet Packages", group: "Plugins/.NET" },
  { id: "dotnet.reloadSolution", title: "Reload Solution", group: "Plugins/.NET" },
  { id: "dotnet.addProject", title: "Add Project from Template…", group: "Plugins/.NET" },
  { id: "dotnet.manageNuGet", title: "Manage NuGet Packages", group: "Plugins/.NET" },
  { id: "dotnet.installSdk", title: "Install .NET SDK…", group: "Plugins/.NET" },
  { id: "dotnet.downloadRoslyn", title: "Download the Roslyn Language Server…", group: "Plugins/.NET" },
  { id: "dotnet.downloadNetcoredbg", title: "Download netcoredbg…", group: "Plugins/.NET" },
]);

registerCommand({ id: "dotnet.buildSolution", title: "Build Solution", category: ".NET", run: needsDotnet(() => void runBuild("build")) });
registerCommand({ id: "dotnet.rebuildSolution", title: "Rebuild Solution", category: ".NET", run: needsDotnet(() => void runBuild("rebuild")) });
registerCommand({ id: "dotnet.cleanSolution", title: "Clean Solution", category: ".NET", run: needsDotnet(() => void runBuild("clean")) });
registerCommand({ id: "dotnet.restore", title: "Restore NuGet Packages", category: ".NET", run: needsDotnet(() => void runBuild("restore")) });
registerCommand({ id: "dotnet.reloadSolution", title: "Reload Solution", category: ".NET", run: needsDotnet(() => void refreshSolution(true)) });
registerCommand({ id: "dotnet.addProject", title: "Add Project from Template…", category: ".NET", run: needsDotnet(openAddProjectDialog) });
registerCommand({
  id: "dotnet.manageNuGet",
  title: "Manage NuGet Packages",
  category: ".NET",
  run: needsDotnet(() => {
    showToolWindow("nuget");
    void refreshUpdates();
  }),
});
registerCommand({ id: "dotnet.installSdk", title: "Install .NET SDK…", category: ".NET", run: () => void installSdk() });
registerCommand({ id: "dotnet.downloadRoslyn", title: "Download the Roslyn Language Server…", category: ".NET", run: () => void installRoslyn() });
registerCommand({ id: "dotnet.downloadNetcoredbg", title: "Download netcoredbg…", category: ".NET", run: () => void installNetcoredbg() });

// Build Project (Ctrl+F9) builds the solution in .NET projects and keeps the other packs'
// Build Project (CForge, JavaForge) elsewhere.
const otherBuild = commands().get("CompileDirty");
registerCommand({
  id: "CompileDirty",
  title: "Build Project",
  category: "Build",
  run: () => {
    if (dotnetActive()) void runBuild("build");
    else if (otherBuild) void otherBuild.run();
    else showDotnetBalloon("Nothing to build", "The project has no build Forge knows.", 4000);
  },
});

registerMainMenuEntries(
  "_Build",
  [
    action("dotnet.buildSolution", "_Build Solution"),
    action("dotnet.rebuildSolution", "_Rebuild Solution"),
    action("dotnet.cleanSolution", "_Clean Solution"),
    action("dotnet.restore", "Restore _NuGet Packages"),
    action("dotnet.reloadSolution", "Reload Sol_ution"),
  ],
  { when: dotnetActive },
);
registerMainMenuEntries("_Tools", [action("dotnet.manageNuGet", "Manage NuGet Packages"), action("dotnet.addProject", "Add Project from Template…")], { when: dotnetActive });

// ---- Startup -----------------------------------------------------------------------------------------

if (typeof document !== "undefined" && !isDialogWindow()) {
  createRoot(() => {
    createEffect(
      on(
        () => [!!workspace() && dotnetActive(), workspace()?.root] as const,
        ([active]) => {
          if (!active) return;
          void refreshSolution();
          void refreshDotnetStatus().then((s) => {
            if (!s || s.remote) return;
            if (!s.host || !s.sdks.length)
              showDotnetBalloon("No .NET SDK found", "Building and running .NET projects needs the .NET SDK.", 12000, [{ label: "Install .NET SDK…", run: () => void installSdk() }]);
            else if (!s.roslyn && getSetting<string>("dotnet.roslyn.autoInstall", "ask") === "ask" && !safeMode())
              showDotnetBalloon("C# language server is not installed", "Code completion, navigation and refactorings for C# need the Roslyn language server.", 12000, [
                { label: "Download…", run: () => void installRoslyn() },
              ]);
          });
        },
      ),
    );
  });
}
