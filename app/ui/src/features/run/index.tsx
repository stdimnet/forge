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
import { RunTabs, RunToolWindow } from "./RunToolWindow";
import { RunConfigurationsDialog } from "./ConfigurationsDialog";
import { ChooserPopup } from "./ChooserPopup";
import { PopupMenuHost } from "./PopupMenu";
import { openChooser, openRunConfigurations } from "./dialogState";
import { selectedConfiguration, useTemporary } from "./configStore";
import { activeSession, closeSession, debugConfiguration, launch, rerunSession, runConfiguration, stopSession } from "./runStore";
import { testGutter } from "./gutter";
import { findTests, scriptConfiguration, testAt, testConfiguration } from "./phpTests";
import "./builtinTypes";
import "./run.css";

export {
  registerRunConfigurationType,
  registerDebugLauncher,
  type RunConfigurationTypeDef,
  type DebugLauncher,
  type DebugLaunchContext,
  type RunHandle,
} from "./types";
export { launch, runConfiguration, debugConfiguration } from "./runStore";

registerToolWindow({
  id: "run",
  title: "Run",
  icon: Play,
  anchor: "bottom",
  order: 10,
  shortcut: "Alt+4",
  component: RunToolWindow,
  headerContent: RunTabs,
});

const isPhp = (path: string) => /\.php$/i.test(path) && !path.endsWith(".blade.php");

registerEditorExtension(({ path }) => (isPhp(path) && /(^|\/)tests?\/|Test\.php$/i.test(path) ? testGutter(path) : null));

/** Temporary configuration for the caret position: test method/class, else the PHP file. */
function contextConfiguration() {
  const a = activeView();
  if (!a || !isPhp(a.path)) return null;
  const doc = a.view.state.doc;
  const line = doc.lineAt(a.view.state.selection.main.head).number - 1;
  const target = testAt(findTests(doc.toString()), line);
  return target ? testConfiguration(target, a.path) : scriptConfiguration(a.path);
}

const cmd = (id: string, title: string, run: () => void | Promise<void>) => registerCommand({ id, title, category: "Run", run });

cmd("run.run", "Run", () => runConfiguration());
cmd("run.debug", "Debug", () => debugConfiguration());
cmd("run.editConfigurations", "Edit Configurations…", () => openRunConfigurations(selectedConfiguration()?.name));
cmd("ChooseRunConfiguration", "Run…", () => openChooser("run"));
cmd("ChooseDebugConfiguration", "Debug…", () => openChooser("debug"));
cmd("Stop", "Stop", () => stopSession());
cmd("Rerun", "Rerun", () => rerunSession());
cmd("RunClass", "Run Context Configuration", () => {
  const cfg = contextConfiguration();
  if (!cfg) return void showBalloon("Nothing to run here", "Open a PHP file or a test.");
  void launch(useTemporary(cfg));
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
        <RunConfigurationsDialog />
        <ChooserPopup />
        <PopupMenuHost />
      </>
    ),
    host,
  );
}
