/**
 * PyForge in the UI (PyCharm's Python support). Everything is computed in Rust
 * (`src/ipc/python.ts`, crates `packs/pyforge/*`); this module plugs it into the
 * contribution points:
 *
 * - the interpreter status bar widget ("Python 3.12 (shop-api)") and its popup to switch,
 * - Settings | Project | Python Interpreter and Settings | Languages & Frameworks | Python,
 * - the Python Packages tool window,
 * - requirements inspections, completion and quick fixes, ▶ on `__main__` and Reformat
 *   Code with ruff format / Black (`editor.ts`),
 * - the "Python" run configuration type,
 * - pytest / Unittests / tox (`testing.ts`), debugpy (`debug.ts`), the Python Console
 *   (`console/`) and the cProfile profiler (`profiler/`).
 */
import { createEffect, createRoot, createSignal, on, Show } from "solid-js";
import { render } from "solid-js/web";
import { FileCode, Package, SquareTerminal } from "lucide-solid";
import { pythonApi, type DetectedPython } from "@/ipc/python";
import type { CommandLine } from "@/ipc/run";
import { workspaceApi } from "@/ipc/workspace";
import { isDialogWindow } from "@/shell/window/mode";
import { registerKeymapActions, showBalloon } from "@/keymap";
import { registerToolWindow } from "@/features/registry";
import { registerSettingsPage } from "@/features/settingsPages";
import { registerStatusItem } from "@/features/statusItems";
import { SchemaPage } from "@/features/settings/SchemaPage";
import { LanguageServersPage } from "@/features/langservers/LanguageServersPage";
import { openSettings } from "@/features/settings/dialogState";
import { item, separator, showContextMenu, submenu, type MenuEntry } from "@/features/contextMenu/state";
import { baseName, registerRunConfigurationType } from "@/features/run/types";
import { registerCommand } from "@/state/commands";
import { showToolWindow, toggleToolWindow } from "@/state/layout";
import { getSetting, saveSettings } from "@/state/settings";
import { workspace } from "@/state/workspace";
import { isPluginActive } from "@/state/project";
import { AddInterpreterDialog, type AddResult } from "./AddInterpreterDialog";
import { InterpreterPage } from "./InterpreterPage";
import { PackagesActions, PackagesView } from "./PackagesView";
import { configuredInterpreters, currentInterpreter, KIND_LABELS, labelOf, packageAction, pythonActive, refreshInterpreter, toSetting } from "./state";
import { isPyproject, isRequirementsPath } from "./editor";
import { showToxPopup } from "./testing";
import "./debug";
import { ConsoleActions, ConsoleView } from "./console/ConsoleView";
import { executeCode, runFileInConsole } from "./console/store";
import { ProfilerActions, ProfilerView } from "./profiler/ProfilerView";
import { loadLatest } from "./profiler/store";
import { registerProfilerSource } from "@/features/profiler";
import { activeView } from "@/features/lsp/navigation";
import { launch } from "@/features/run/runStore";
import { selectedConfiguration } from "@/features/run/configStore";
import { openRunConfigurations } from "@/features/run/dialogState";
import { action, registerMainMenuEntries } from "@/features/mainMenu/model";
import "./python.css";
import "./profiler/profiler.css";
import { t, t as tr } from "@/i18n";

// ---- Settings -----------------------------------------------------------------------------

registerSettingsPage({
  id: "project.python-interpreter",
  title: t("settings.page.project.python-interpreter"),
  group: "Project",
  order: 0,
  keywords: ["python", "interpreter", "venv", "virtualenv", "uv", "poetry", "conda", "pipenv", "hatch", "pyenv", "docker", "ssh", "wsl", "packages"],
  component: InterpreterPage,
});

registerSettingsPage({
  id: "lang.python",
  title: t("settings.page.lang.python"),
  group: "Languages & Frameworks",
  order: 25,
  keywords: ["python", "ruff", "black", "basedpyright", "pyright", "ty", "formatter", "pypi", "requirements", "inspections"],
  component: () => (
    <div class="py-settings-page">
      <SchemaPage pageId="lang.python" />
      <h3 class="set-section-title">{tr("langs.python.languageServers")}</h3>
      <LanguageServersPage groups={["Python"]} />
    </div>
  ),
});

registerSettingsPage({
  id: "tools.python-integrated-tools",
  title: t("settings.page.tools.python-integrated-tools"),
  group: "Tools",
  order: 40,
  keywords: ["pytest", "unittest", "tox", "test runner", "default test runner", "junit", "report log", "profiler", "cprofile"],
  component: () => <SchemaPage pageId="tools.python-integrated-tools" />,
});

registerSettingsPage({
  id: "build.python-debugger",
  title: t("settings.page.build.python-debugger"),
  group: "Build, Execution, Deployment",
  parent: "build.debugger",
  order: 20,
  keywords: ["debugpy", "justMyCode", "just my code", "subprocess", "python debug"],
  component: () => <SchemaPage pageId="build.python-debugger" />,
});

registerSettingsPage({
  id: "build.python-console",
  title: t("settings.page.build.python-console"),
  group: "Build, Execution, Deployment",
  order: 25,
  keywords: ["python console", "ipython", "repl", "interpreter options"],
  component: () => <SchemaPage pageId="build.python-console" />,
});

// ---- Add interpreter and the status bar popup -----------------------------------------------

const [dialog, setDialog] = createSignal(false);

/** Store a result of the Add Python Interpreter dialog right away. */
async function storeResult(r: AddResult): Promise<void> {
  const changes: Record<string, unknown> = { "python.interpreter": r.select };
  if (r.add) {
    const list = getSetting<Record<string, string>[]>("python.projectInterpreters", []) ?? [];
    changes["python.projectInterpreters"] = [...list.filter((t) => t.name !== r.add!.name), toSetting(r.add)];
  }
  await saveSettings(changes as Parameters<typeof saveSettings>[0]);
  await refreshInterpreter();
  showBalloon(tr("langs.python.pythonInterpreter2"), tr("langs.python.theProjectNowUses", { value: currentInterpreter()?.label ?? r.select }));
}

export function openAddInterpreter(): void {
  setDialog(true);
}

async function select(value: string): Promise<void> {
  await saveSettings({ "python.interpreter": value });
  await refreshInterpreter();
}

/** The interpreter popup: configured interpreters and detected environments to switch to. */
export async function showInterpreterPopup(x: number, y: number): Promise<void> {
  const detected = await pythonApi.detect().catch((): DetectedPython[] => []);
  const cur = currentInterpreter()?.interpreter;
  const isCurrent = (path: string, name?: string) => !!cur && (cur.path === path || (!!name && cur.name === name));
  const entries: MenuEntry[] = [];
  for (const i of configuredInterpreters()) {
    entries.push(item(`${labelOf(i)}  [${KIND_LABELS[i.kind]}]`, () => void select(i.name), undefined, { checked: isCurrent(i.path, i.name) }));
  }
  const project = detected.filter((d) => d.project);
  const others = detected.filter((d) => !d.project);
  for (const d of project) entries.push(item(`${labelOf(d)}  ${d.path}`, () => void select(d.path), undefined, { checked: isCurrent(d.path) }));
  if (others.length) {
    if (entries.length) entries.push(separator);
    for (const d of others.slice(0, 12)) entries.push(item(`${labelOf(d)}  ${d.path}`, () => void select(d.path), undefined, { checked: isCurrent(d.path) }));
  }
  if (entries.length) entries.push(separator);
  entries.push(
    submenu("Add New Interpreter", [
      item("Add Local Interpreter…", openAddInterpreter),
      item("On Docker…", openAddInterpreter),
      item("On Docker Compose…", openAddInterpreter),
      item("On SSH…", openAddInterpreter),
      item("On WSL…", openAddInterpreter),
    ]),
    item("Interpreter Settings…", () => openSettings("project.python-interpreter")),
    item("Manage Packages…", () => showToolWindow("pythonPackages")),
  );
  showContextMenu(entries, x, y);
}

function InterpreterStatus() {
  return (
    <button
      class="status-item py-status"
      data-status="python-interpreter"
      title={currentInterpreter() ? tr("langs.python.clickToSwitchThePython", { path: currentInterpreter()!.interpreter.path }) : tr("langs.python.noPythonInterpreterClickTo")}
      onClick={(e) => {
        const r = e.currentTarget.getBoundingClientRect();
        void showInterpreterPopup(r.left, r.top - 4);
      }}
    >
      <FileCode size={12} />
      <span>{currentInterpreter()?.label ?? tr("langs.python.noInterpreter")}</span>
    </button>
  );
}

registerStatusItem({ id: "python.interpreter", align: "right", order: 88, component: InterpreterStatus, when: pythonActive });

// ---- Python Packages tool window ---------------------------------------------------------

registerToolWindow({
  id: "pythonPackages",
  title: t("toolwindow.pythonPackages.title"),
  icon: Package,
  anchor: "bottom",
  order: 45,
  component: PackagesView,
  actions: PackagesActions,
  when: pythonActive,
});

// ---- Python Console and Profiler -----------------------------------------------------------

const toolsActive = () => pythonActive() && isPluginActive("forge.python-tools");

registerToolWindow({
  id: "pythonConsole",
  title: t("toolwindow.pythonConsole.title"),
  icon: SquareTerminal,
  anchor: "bottom",
  order: 46,
  component: ConsoleView,
  actions: ConsoleActions,
  when: toolsActive,
});

// The shared Profiler tool window (features/profiler) shows cProfile snapshots.
registerProfilerSource({
  id: "python",
  label: "cProfile",
  active: toolsActive,
  component: ProfilerView,
  actions: ProfilerActions,
  onActivate: () => void loadLatest(),
  order: 10,
});

registerKeymapActions([
  {
    id: "ExecuteInPyConsoleAction",
    title: "Run Selection in Python Console",
    group: "Plugins/Python",
    context: "editor",
    shortcuts: { $default: ["Alt+Shift+E"], macOS: ["Alt+Shift+E"] },
  },
  { id: "RunFileInPythonConsole", title: "Run File in Python Console", group: "Plugins/Python" },
  { id: "ActivatePythonConsoleToolWindow", title: "Python Console", group: "Tool Windows" },
  { id: "python.profile", title: "Profile", group: "Plugins/Python" },
  { id: "python.runToxEnvironment", title: "Run tox Environment…", group: "Plugins/Python" },
]);

/** The selection, or the caret's line without one. */
function selectionOrLine(): string | null {
  const a = activeView();
  if (!a) return null;
  const st = a.view.state;
  const r = st.selection.main;
  return r.empty ? st.doc.lineAt(r.head).text : st.sliceDoc(r.from, r.to);
}

registerCommand({
  id: "ExecuteInPyConsoleAction",
  title: "Run Selection in Python Console",
  category: "Python",
  run: () => {
    const code = selectionOrLine();
    if (!code?.trim()) return void showBalloon(tr("langs.python.nothingToRun"), tr("langs.python.selectPythonCodeOrPlace"));
    void executeCode(code);
  },
});
registerCommand({
  id: "RunFileInPythonConsole",
  title: "Run File in Python Console",
  category: "Python",
  run: () => {
    const a = activeView();
    if (!a || !/\.pyw?$/i.test(a.path)) return void showBalloon(tr("langs.python.runFileInPythonConsole"), tr("langs.python.openAPythonFile"));
    void runFileInConsole(a.path);
  },
});
registerCommand({ id: "ActivatePythonConsoleToolWindow", title: "Python Console", category: "View", run: () => toggleToolWindow("pythonConsole") });
registerCommand({
  id: "python.profile",
  title: "Profile",
  category: "Run",
  run: () => {
    const cfg = selectedConfiguration();
    if (!cfg) return openRunConfigurations();
    if (cfg.type !== "python") return void showBalloon(tr("langs.python.cannotProfile", { name: cfg.name }), tr("langs.python.profileRunsPythonConfigurationsWith"));
    void launch(cfg, { extra: { profile: true } });
  },
});
registerCommand({ id: "python.runToxEnvironment", title: "Run tox Environment…", category: "Python", run: () => void showToxPopup() });

registerMainMenuEntries("Run", [action("python.profile", "Pro_file")], { after: "Debug", when: toolsActive });
registerMainMenuEntries("Tools", [action("ActivatePythonConsoleToolWindow", "Python Conso_le"), action("python.runToxEnvironment", "Run to_x Environment...")], {
  when: toolsActive,
});

// ---- Commands --------------------------------------------------------------------------

registerKeymapActions([
  { id: "ActivatePythonPackagesToolWindow", title: "Python Packages", group: "Tool Windows" },
  { id: "python.selectInterpreter", title: "Switch Python Interpreter…", group: "Plugins/Python" },
  { id: "python.addInterpreter", title: "Add Python Interpreter…", group: "Plugins/Python" },
  { id: "python.installRequirements", title: "Install Requirements", group: "Plugins/Python" },
]);

registerCommand({
  id: "ActivatePythonPackagesToolWindow",
  title: "Python Packages",
  category: "View",
  run: () => toggleToolWindow("pythonPackages"),
});
registerCommand({
  id: "python.selectInterpreter",
  title: "Switch Python Interpreter…",
  category: "Python",
  run: () => {
    const el = document.querySelector<HTMLElement>('[data-status="python-interpreter"]');
    const r = el?.getBoundingClientRect();
    void showInterpreterPopup(r?.left ?? window.innerWidth - 320, (r?.top ?? window.innerHeight - 30) - 4);
  },
});
registerCommand({ id: "python.addInterpreter", title: "Add Python Interpreter…", category: "Python", run: openAddInterpreter });
registerCommand({
  id: "python.installRequirements",
  title: "Install Requirements",
  category: "Python",
  run: () => void installRequirements(),
});

/** Install every declared dependency that is not satisfied (requirements files, pyproject.toml). */
async function installRequirements(): Promise<void> {
  const files = await workspaceApi.listDir("").catch(() => []);
  const targets = files.map((f) => f.path).filter((p) => isRequirementsPath(p) || isPyproject(p));
  const specs: string[] = [];
  for (const path of targets) {
    const text = await workspaceApi.readFile(path).catch(() => "");
    for (const p of await pythonApi.inspect(path, text).catch(() => [])) {
      for (const f of p.fixes) if (f.kind === "installRequirements" && f.requirements.length === 1) specs.push(f.requirements[0]);
    }
  }
  if (!specs.length) {
    showBalloon(tr("langs.python.allRequirementsAreSatisfied"));
    return;
  }
  await packageAction("install", [...new Set(specs)]);
}

// ---- Run configurations ------------------------------------------------------------------

registerRunConfigurationType({
  id: "python",
  name: "Python",
  description: "Run a Python script or module (python -m) with the project's interpreter.",
  icon: FileCode,
  color: "var(--info)",
  pluginId: "forge.python-env",
  interpreter: "custom",
  defaultInterpreter: () => currentInterpreter()?.interpreter.path || getSetting<string>("python.interpreter", "") || "python3",
  argumentsLabel: "Script parameters",
  defaults: () => ({ options: { target: "script" } }),
  fields: [
    {
      key: "target",
      label: "Run",
      kind: "segmented",
      options: [
        { value: "script", label: "Script path" },
        { value: "module", label: "Module name" },
      ],
      default: "script",
    },
    { key: "script", label: "Script / module", kind: "path", required: true, placeholder: "main.py or package.module" },
  ],
  suggestName: (cfg) => {
    const s = cfg.options?.script;
    return cfg.options?.target === "module" ? s : baseName(s)?.replace(/\.pyw?$/, "");
  },
});

// Package commands (pip / uv / Poetry / Pipenv) built in Rust, started from the UI.
registerRunConfigurationType({
  id: "python-command",
  name: "Python Packages",
  icon: Package,
  hidden: true,
  fields: [],
  argumentsLabel: false,
  buildCommand: (cfg) => JSON.parse(cfg.options?.commandLine ?? "{}") as CommandLine,
});

// ---- Startup -----------------------------------------------------------------------------

if (typeof document !== "undefined" && !isDialogWindow()) {
  createRoot(() => {
    createEffect(
      on(
        () => [!!workspace() && pythonActive(), workspace()?.root, getSetting<string>("python.interpreter", ""), JSON.stringify(getSetting("python.projectInterpreters", []))] as const,
        ([on]) => {
          if (on) void refreshInterpreter();
        },
      ),
    );
  });
  const host = document.createElement("div");
  document.body.append(host);
  render(
    () => (
      <Show when={dialog()}>
        <AddInterpreterDialog
          onDone={(r) => {
            setDialog(false);
            void storeResult(r);
          }}
          onClose={() => setDialog(false)}
        />
      </Show>
    ),
    host,
  );
}
