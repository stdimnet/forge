/**
 * Run/Debug: configurations (toolbar widget, Run/Debug Configurations dialog, Run… chooser),
 * the Run tool window (Alt+4) with console and test tree, PHPUnit gutter icons, and the
 * contribution points `registerRunConfigurationType` and `registerDebugLauncher`.
 */
import { render } from "solid-js/web";
import { Play } from "lucide-solid";
import { registerToolWindow } from "@/features/registry";
import { registerEditorExtension } from "@/editor/extensions";
import { registerCommand } from "@/state/commands";
import { activeView } from "@/features/lsp/navigation";
import { showBalloon } from "@/keymap";
import { RunTabs, RunToolWindow, showTestHistory } from "./RunToolWindow";
import { RunConfigurationsDialog } from "./ConfigurationsDialog";
import { ChooserPopup } from "./ChooserPopup";
import { PopupMenuHost } from "./PopupMenu";
import { openChooser, openRunConfigurations, RUN_CONFIGURATIONS_DIALOG, type DialogRequest } from "./dialogState";
import { loadConfigurations, reloadConfigurations, selectedConfiguration, useTemporary } from "./configStore";
import { registerDialog, usesDialogWindows } from "@/shell/window/dialogs";
import {
  activeSession,
  closeSession,
  debugConfiguration,
  launch,
  rerunSession,
  runConfiguration,
  toggleAutoTest,
  runningFor,
  runWithCoverage,
  stopConfiguration,
  stopSession,
} from "./runStore";
import { testGutter, testsOf } from "./gutter";
import { testAt } from "./tests";
import { COMPOUND } from "./types";
import { scriptConfiguration } from "./phpTests";
import { isFrameworkTestFile } from "./frameworkFiles";
import type { RunConfiguration } from "@/ipc/run";
import "./builtinTypes";
import "./testFrameworkTypes";
import "./run.css";
import { t } from "@/i18n";

export {
  registerRunConfigurationType,
  registerDebugLauncher,
  type RunConfigurationTypeDef,
  type DebugLauncher,
  type DebugLaunchContext,
  type RunHandle,
} from "./types";
export {
  launch,
  runConfiguration,
  runWithCoverage,
  onCoverageSuite,
  onProfileSnapshot,
  debugConfiguration,
  stopConfiguration,
  registerBeforeLaunchHandler,
  openInBrowser,
  type BeforeLaunchHandler,
} from "./runStore";
export { COMPOUND } from "./types";
export { registerTestFileMatcher } from "./frameworkFiles";

registerToolWindow({
  id: "run",
  title: t("toolwindow.run.title"),
  icon: Play,
  anchor: "bottom",
  order: 10,
  shortcut: "Alt+4",
  component: RunToolWindow,
  headerContent: RunTabs,
});

const isPhp = (path: string) => /\.php$/i.test(path) && !path.endsWith(".blade.php");
/**
 * Files that may hold tests: PHP test, spec and feature folders, `*Test`, `*Cest`, `*Cept`,
 * `*Spec`, and the `testFiles` globs of manifest test frameworks.
 */
const mayHoldTests = (path: string) =>
  /\.feature$/i.test(path) ||
  (isPhp(path) && /(^|\/)(tests?|spec|features)\/|(Test|Cest|Cept|Spec)\.php$/i.test(path)) ||
  isFrameworkTestFile(path);

registerEditorExtension(({ path }) => (mayHoldTests(path) ? testGutter(path) : null));

const contextFallbacks: ((path: string) => RunConfiguration | null)[] = [];

/**
 * Run Context Configuration outside a test: a pack's configuration for the file (a Python
 * script), tried after the tests of the file.
 */
export function registerContextConfiguration(fallback: (path: string) => RunConfiguration | null): void {
  contextFallbacks.push(fallback);
}

/**
 * Temporary configuration for the caret position: the test at or above it, else the whole
 * test file, else the PHP file as a script (or a pack's configuration for the file).
 */
function contextConfiguration() {
  const a = activeView();
  if (!a) return null;
  if (!(isPhp(a.path) || /\.feature$/i.test(a.path) || isFrameworkTestFile(a.path))) {
    for (const f of contextFallbacks) {
      const cfg = f(a.path);
      if (cfg) return cfg;
    }
    return null;
  }
  const state = a.view.state;
  const line = state.doc.lineAt(state.selection.main.head).number - 1;
  const tests = testsOf(state);
  const target = testAt(tests.targets, line) ?? tests.targets[0];
  if (target) return target.configuration;
  if (tests.file) return tests.file;
  if (isPhp(a.path)) return scriptConfiguration(a.path);
  for (const f of contextFallbacks) {
    const cfg = f(a.path);
    if (cfg) return cfg;
  }
  return null;
}

registerDialog<DialogRequest>({
  id: RUN_CONFIGURATIONS_DIALOG,
  title: "Run/Debug Configurations",
  width: 1000,
  height: 660,
  minWidth: 720,
  minHeight: 420,
  component: RunConfigurationsDialog,
  // A dialog window loads the project's configurations before it shows them.
  prepare: () => (usesDialogWindows() ? loadConfigurations() : undefined),
  // The dialog saved in its own window: show the new configurations here.
  onMessage: () => {
    if (usesDialogWindows()) void reloadConfigurations();
  },
});

const cmd = (id: string, title: string, run: () => void | Promise<void>) => registerCommand({ id, title, category: "Run", run });

cmd("run.run", "Run", () => runConfiguration());
cmd("run.debug", "Debug", () => debugConfiguration());
cmd("run.editConfigurations", "Edit Configurations…", () => openRunConfigurations(selectedConfiguration()?.name));
cmd("ChooseRunConfiguration", "Run…", () => openChooser("run"));
cmd("ChooseDebugConfiguration", "Debug…", () => openChooser("debug"));
cmd("Stop", "Stop", () => {
  // A running compound stops all its members.
  const selected = selectedConfiguration();
  if (selected?.type === COMPOUND && runningFor(selected)) return stopConfiguration(selected);
  return stopSession();
});
cmd("Rerun", "Rerun", () => rerunSession());
cmd("ToggleAutoTestAction", "Toggle Auto-Test", () => toggleAutoTest());
cmd("ShowTestHistory", "Test History…", () => showTestHistory(window.innerWidth / 2 - 160, window.innerHeight / 3));
cmd("RunClass", "Run Context Configuration", () => {
  const cfg = contextConfiguration();
  if (!cfg) return void showBalloon("Nothing to run here", "Open a PHP file or a test.");
  void launch(useTemporary(cfg));
});
cmd("Coverage", "Run with Coverage", () => runWithCoverage());
cmd("RunCoverage", "Run Context Configuration with Coverage", () => {
  const cfg = contextConfiguration();
  if (!cfg) return void showBalloon("Nothing to run here", "Open a test file.");
  void runWithCoverage(useTemporary(cfg));
});
cmd("DebugClass", "Debug Context Configuration", () => {
  const cfg = contextConfiguration();
  if (!cfg) return void showBalloon("Nothing to debug here", "Open a PHP file or a test.");
  void debugConfiguration(useTemporary(cfg));
});
cmd("CloseActiveTab", "Close Active Tool Window Tab", async () => {
  const panel = document.activeElement?.closest<HTMLElement>("[data-tool-window-panel]")?.dataset.toolWindowPanel;
  if (panel === "run") {
    const s = activeSession();
    if (s) await closeSession(s.id);
  } else if (panel === "terminal") {
    const { closeTerminalTab } = await import("@/features/terminal/TerminalView");
    const { activeTab } = await import("@/features/terminal/store");
    const id = activeTab();
    if (id) closeTerminalTab(id);
  }
});

if (typeof document !== "undefined") {
  const host = document.createElement("div");
  host.id = "run-overlay";
  document.body.appendChild(host);
  render(
    () => (
      <>
        <ChooserPopup />
        <PopupMenuHost />
      </>
    ),
    host,
  );
}
