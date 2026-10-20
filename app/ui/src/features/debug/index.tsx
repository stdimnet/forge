/**
 * PHP debugger (Xdebug): Debug tool window (Alt+5), listening toggle in the main toolbar,
 * breakpoints and execution point in the editor, Evaluate / Breakpoints / Incoming
 * Connection dialogs, and the PHP > Debug and PHP > Servers settings pages.
 *
 * Run configurations can start a debug session through `debugLauncher` (see below).
 */
import { createEffect, createRoot, on, Show } from "solid-js";
import { isDialogWindow } from "@/shell/window/mode";
import { render } from "solid-js/web";
import { Bug, PhoneCall, PhoneOff } from "lucide-solid";
import { registerToolWindow } from "@/features/registry";
import { registerStatusItem } from "@/features/statusItems";
import { registerToolbarItem } from "@/features/toolbarItems";
import { registerSettingsPage } from "@/features/settingsPages";
import { SchemaPage } from "@/features/settings/SchemaPage";
import { registerEditorExtension } from "@/editor/extensions";
import { registerCommand } from "@/state/commands";
import { registerKeymapActions } from "@/keymap/store";
import { showToolWindow } from "@/state/layout";
import { workspace } from "@/state/workspace";
import { activeView } from "@/features/lsp/navigation";
import { debugApi } from "./api";
import { DebugActions, DebugView } from "./DebugView";
import { DebugOverlays } from "./DebugOverlays";
import { debugEditorExtensions } from "./editorDebug";
import { expressionAt } from "./format";
import { setBreakpointsDialog, setEvaluateRequest } from "./overlays";
import { ServersPage } from "./ServersPage";
import {
  activeSession,
  addWatch,
  breakpointsMuted,
  executionPoint,
  listening,
  pause,
  pausedSession,
  port,
  reloadDebugState,
  runToCursor,
  showLine,
  startDebugStore,
  step,
  stop,
  toggleListening,
  toggleLineBreakpoint,
  toggleMuted,
} from "./store";
import { registerMainMenuToggle } from "@/features/mainMenu/model";
import "./debug.css";

registerToolWindow({
  id: "debug",
  title: "Debug",
  icon: Bug,
  anchor: "bottom",
  order: 20,
  shortcut: "Alt+5",
  component: DebugView,
  actions: DebugActions,
});

registerEditorExtension(({ path }) => debugEditorExtensions(path));

// ---- toolbar + status bar -------------------------------------------------------------

function ListenButton() {
  return (
    <button
      class="icon-btn dbg-listen-btn"
      classList={{ "dbg-listening": listening() }}
      title={listening() ? "Stop Listening for PHP Debug Connections" : "Start Listening for PHP Debug Connections"}
      aria-pressed={listening()}
      onClick={() => void toggleListening()}
      data-action="PhpListenDebugAction"
    >
      {listening() ? <PhoneCall size={16} /> : <PhoneOff size={16} />}
    </button>
  );
}

function DebugStatusItem() {
  const label = () => {
    const s = activeSession();
    if (s?.status === "paused") return `Debugging: suspended at ${s.location?.file?.split("/").pop() ?? "?"}:${s.location?.line ?? ""}`;
    if (s?.status === "running") return "Debugging: running";
    return `Xdebug: listening on ${port()}`;
  };
  return (
    <Show when={listening() || activeSession()}>
      <button
        class="status-item dbg-status"
        classList={{ paused: !!pausedSession() }}
        title={listening() ? "Listening for PHP debug connections. Click to stop." : "Debug session"}
        onClick={() => (activeSession() ? showToolWindow("bottom", "debug") : void toggleListening())}
      >
        <PhoneCall size={12} />
        {label()}
      </button>
    </Show>
  );
}

registerToolbarItem({ id: "php.debug.listen", order: 10, component: ListenButton });
registerStatusItem({ id: "php.debug", align: "right", order: -20, component: DebugStatusItem });

// ---- settings -------------------------------------------------------------------------

registerSettingsPage({
  id: "lang.php.debug",
  title: "Debug",
  group: "Languages & Frameworks",
  parent: "lang.php",
  order: 10,
  keywords: ["xdebug", "port", "9003", "break at first line", "dbgp", "max children", "max depth"],
  component: () => <SchemaPage pageId="lang.php.debug" />,
});

registerSettingsPage({
  id: "lang.php.servers",
  title: "Servers",
  group: "Languages & Frameworks",
  parent: "lang.php",
  order: 20,
  keywords: ["path mappings", "host", "docker", "remote", "xdebug", "PHP_IDE_CONFIG"],
  component: ServersPage,
});

// ---- actions (all bound through the keymap) ---------------------------------------------

registerKeymapActions([
  {
    id: "PhpListenDebugAction",
    title: "Start Listening for PHP Debug Connections",
    group: "Main Menu/Run",
    shortcuts: { $default: [], macOS: [] },
  },
]);

/** File and 1-based caret line of the active editor. */
function caret(): { path: string; line: number; text: string; col: number } | null {
  const a = activeView();
  if (!a) return null;
  const head = a.view.state.selection.main.head;
  const line = a.view.state.doc.lineAt(head);
  return { path: a.path, line: line.number, text: line.text, col: head - line.from };
}

function selectionOrWord(): string {
  const a = activeView();
  if (!a) return "";
  const { from, to } = a.view.state.selection.main;
  if (from !== to) return a.view.state.sliceDoc(from, to);
  const c = caret();
  return c ? expressionAt(c.text, c.col) : "";
}

const actions: { id: string; title: string; run: () => void | Promise<void> }[] = [
  { id: "PhpListenDebugAction", title: "Start/Stop Listening for PHP Debug Connections", run: toggleListening },
  { id: "Resume", title: "Resume Program", run: () => step("run") },
  { id: "Pause", title: "Pause Program", run: pause },
  { id: "StepOver", title: "Step Over", run: () => step("stepOver") },
  { id: "StepInto", title: "Step Into", run: () => step("stepInto") },
  { id: "SmartStepInto", title: "Smart Step Into", run: () => step("stepInto") },
  { id: "ForceStepInto", title: "Force Step Into", run: () => step("forceStepInto") },
  { id: "StepOut", title: "Step Out", run: () => step("stepOut") },
  {
    id: "RunToCursor",
    title: "Run to Cursor",
    run: () => {
      const c = caret();
      if (c) return runToCursor(c.path, c.line);
    },
  },
  {
    id: "ForceRunToCursor",
    title: "Force Run to Cursor",
    run: () => {
      const c = caret();
      if (c) return runToCursor(c.path, c.line);
    },
  },
  {
    id: "EvaluateExpression",
    title: "Evaluate Expression…",
    run: () => {
      if (pausedSession()) setEvaluateRequest({ expression: selectionOrWord() });
    },
  },
  {
    id: "QuickEvaluateExpression",
    title: "Quick Evaluate Expression",
    run: () => {
      const e = selectionOrWord();
      if (pausedSession() && e) setEvaluateRequest({ expression: e });
    },
  },
  {
    id: "Debugger.AddToWatch",
    title: "Add to Watches",
    run: () => {
      const e = selectionOrWord();
      if (e) addWatch(e);
    },
  },
  {
    id: "ShowExecutionPoint",
    title: "Show Execution Point",
    run: () => {
      const p = executionPoint();
      if (p) return showLine(p.file, p.line);
    },
  },
  {
    id: "ToggleLineBreakpoint",
    title: "Toggle Line Breakpoint",
    run: () => {
      const c = caret();
      if (c && /\.(php|phtml|inc)$/i.test(c.path)) return toggleLineBreakpoint(c.path, c.line);
    },
  },
  { id: "ViewBreakpoints", title: "View Breakpoints…", run: () => setBreakpointsDialog({}) },
  { id: "XDebugger.MuteBreakpoints", title: "Mute Breakpoints", run: toggleMuted },
];
for (const a of actions) registerCommand({ ...a, category: "Debug" });
registerMainMenuToggle("PhpListenDebugAction", listening);
registerMainMenuToggle("XDebugger.MuteBreakpoints", breakpointsMuted);

/**
 * "Stop" (Ctrl+F2) stops the active debug session; otherwise it falls back to a stop
 * handler registered by run configurations (`debugLauncher.setStopFallback`).
 */
let stopFallback: (() => void | Promise<void>) | null = null;
registerCommand({
  id: "Stop",
  title: "Stop",
  category: "Run",
  run: async () => {
    if (!(await stop())) await stopFallback?.();
  },
});

// ---- integration API for run configurations ----------------------------------------------

/**
 * Hook for Debug run configurations (PHP Script, PHPUnit, built-in server): make sure Forge
 * listens, then start the process with `env` so Xdebug connects back.
 */
export const debugLauncher = {
  /** Start listening if needed and return the environment for the PHP process. */
  async prepare(serverName?: string): Promise<Record<string, string>> {
    if (!listening()) await debugApi.listenStart();
    return debugApi.launchEnv(serverName);
  },
  isListening: listening,
  setStopFallback(fn: (() => void | Promise<void>) | null) {
    stopFallback = fn;
  },
};

// ---- startup --------------------------------------------------------------------------

if (typeof document !== "undefined") {
  const host = document.createElement("div");
  host.id = "debug-overlays";
  document.body.append(host);
  render(DebugOverlays, host);
  void startDebugStore();
  // Breakpoints and servers belong to the project: reload when another one opens.
  if (!isDialogWindow()) {
    createRoot(() => createEffect(on(() => workspace()?.root, () => void reloadDebugState(), { defer: true })));
  }
}
