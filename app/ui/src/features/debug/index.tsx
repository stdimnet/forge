/**
 * The debugger: Debug tool window (Alt+5), breakpoints and execution point in the editor,
 * Evaluate / Breakpoints / Attach dialogs. Xdebug (PHP) sessions and Debug Adapter Protocol
 * sessions of every other language share all of it; PHP adds the listening toggle, the
 * Incoming Connection dialog and the PHP > Debug and PHP > Servers settings pages.
 *
 * Run configurations start Xdebug sessions through `debugLauncher` and DAP sessions through
 * the adapters registered with `registerDebugAdapter` (see `adapters.ts`).
 */
import { createEffect, createRoot, on, Show } from "solid-js";
import { isDialogWindow } from "@/shell/window/mode";
import { render } from "solid-js/web";
import { Bug, Cpu, PhoneCall, PhoneOff } from "lucide-solid";
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
import { setBreakpointsDialog, setEvaluateRequest, setValidationDialog } from "./overlays";
import { copyValue, editSelectedValue } from "./VariablesTree";
import { setAttachDialog } from "./AttachDialogs";
import { adapterForRunType, debugAdapters, defaultLaunchRequest, isDebuggableFile, refreshDebugAdapters } from "./adapters";
import { dapApi } from "./dapApi";
import { registerDebugLauncher } from "@/features/run";
import { registerRunConfigurationType } from "@/features/run/types";
import { action, registerMainMenuEntries } from "@/features/mainMenu/model";
import { ServersPage } from "./ServersPage";
import {
  activeSession,
  addWatch,
  addWatchpoint,
  breakpointsMuted,
  executionPoint,
  listening,
  listeningVia,
  pause,
  pausedSession,
  port,
  reloadDebugState,
  restartSession,
  runToCursor,
  selectedVariable,
  showLine,
  startDebugStore,
  step,
  stop,
  toggleListening,
  toggleLineBreakpoint,
  toggleMuted,
  toggleTemporaryLineBreakpoint,
} from "./store";
import { registerMainMenuToggle } from "@/features/mainMenu/model";
import { balloonsFor } from "@/keymap/dispatch";

export { registerDebugAdapter, debugAdapters, isDebuggableFile, type DebugAdapterRegistration } from "./adapters";
export { registerDebugTab, type DebugTabDef } from "./debugTabs";
export { dapApi, type LaunchRequest } from "./dapApi";

const showDebuggerBalloon = balloonsFor("Debugger");
import "./debug.css";
import { t } from "@/i18n";

registerToolWindow({
  id: "debug",
  title: t("toolwindow.debug.title"),
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
      title={listening() ? t("debug.main.stopListeningForPhpDebug") : t("debug.main.startListeningForPhpDebug")}
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
    if (s?.status === "paused") return t("debug.main.suspendedAt", { file: s.location?.file?.split("/").pop() ?? "?", line: String(s.location?.line ?? "") });
    if (s?.status === "running") return t("debug.main.running");
    const via = listeningVia();
    // Remote project: the host's port, forwarded over SSH (see the tooltip).
    return via ? `Xdebug: ${via.replace(/:\d+$/, "")}:${port()}` : t("debug.main.xdebugListeningOnPort", { port: port() });
  };
  const title = () => {
    if (!listening()) return "Debug session";
    const via = listeningVia();
    return via
      ? t("debug.main.listeningForPhpDebugConnections", { port: port(), via })
      : "Listening for PHP debug connections. Click to stop.";
  };
  return (
    <Show when={listening() || activeSession()}>
      <button
        class="status-item dbg-status"
        classList={{ paused: !!pausedSession() }}
        title={title()}
        onClick={() => (activeSession() ? showToolWindow("debug") : void toggleListening())}
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
  title: t("settings.page.lang.php.debug"),
  group: "Languages & Frameworks",
  parent: "lang.php",
  order: 10,
  keywords: ["xdebug", "port", "9003", "break at first line", "dbgp", "max children", "max depth"],
  component: () => <SchemaPage pageId="lang.php.debug" />,
});

registerSettingsPage({
  id: "build.debugger",
  title: t("settings.page.build.debugger"),
  group: "Build, Execution, Deployment",
  order: 5,
  keywords: ["dap", "debug adapter", "lldb", "gdb", "lldb-dap", "memory", "inline values", "attach"],
  component: () => <SchemaPage pageId="build.debugger" />,
});

registerSettingsPage({
  id: "lang.php.servers",
  title: t("settings.page.lang.php.servers"),
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
  {
    id: "WebServerDebugValidation",
    title: "Web Server Debug Validation",
    group: "Main Menu/Run",
    shortcuts: { $default: [], macOS: [] },
  },
  { id: "Forge.AttachToDapServer", title: "Attach to DAP Server…", group: "Main Menu/Run", shortcuts: { $default: [], macOS: [] } },
  { id: "Forge.DebugCoreDump", title: "Debug Core Dump…", group: "Main Menu/Run", shortcuts: { $default: [], macOS: [] } },
  { id: "Forge.Debugger.Rerun", title: "Rerun Debug Session", group: "Debugger Actions", shortcuts: { $default: [], macOS: [] } },
  { id: "Forge.Debugger.StepOverInstruction", title: "Step Over Instruction", group: "Debugger Actions", shortcuts: { $default: [], macOS: [] } },
  { id: "Forge.Debugger.StepIntoInstruction", title: "Step Into Instruction", group: "Debugger Actions", shortcuts: { $default: [], macOS: [] } },
  { id: "Forge.Debugger.AddWatchpoint", title: "Add Watchpoint", group: "Debugger Actions", shortcuts: { $default: [], macOS: [] } },
  { id: "Forge.Debugger.JumpToLine", title: "Set Execution Point to Cursor", group: "Debugger Actions", shortcuts: { $default: [], macOS: [] } },
]);

/** Focus is in the Debug tool window (its variables tree, watches or console). */
const inDebugWindow = () => !!document.activeElement?.closest('[data-tool-window-panel="debug"], .dbg-view');

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
      const v = selectedVariable();
      if (inDebugWindow() && v && !v.watch) return addWatch(v.property.fullname);
      const e = selectionOrWord();
      if (e) addWatch(e);
    },
  },
  {
    id: "XDebugger.SetValue",
    title: "Set Value…",
    run: () => {
      if (!pausedSession() || !selectedVariable()) return;
      showToolWindow("debug");
      editSelectedValue();
    },
  },
  {
    id: "XDebugger.CopyValue",
    title: "Copy Value",
    run: () => {
      const v = selectedVariable();
      if (v) return copyValue(v);
    },
  },
  {
    id: "ToggleTemporaryLineBreakpoint",
    title: "Toggle Temporary Line Breakpoint",
    run: () => {
      const c = caret();
      if (c && isDebuggableFile(c.path)) return toggleTemporaryLineBreakpoint(c.path, c.line);
    },
  },
  { id: "WebServerDebugValidation", title: "Web Server Debug Validation", run: () => setValidationDialog(true) },
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
      if (c && isDebuggableFile(c.path)) return toggleLineBreakpoint(c.path, c.line);
    },
  },
  { id: "ViewBreakpoints", title: "View Breakpoints…", run: () => setBreakpointsDialog({}) },
  { id: "XDebugger.MuteBreakpoints", title: "Mute Breakpoints", run: toggleMuted },
  // Debug adapter sessions.
  { id: "XDebugger.AttachToProcess", title: "Attach to Process…", run: () => openAttach("process") },
  { id: "Forge.AttachToDapServer", title: "Attach to DAP Server…", run: () => openAttach("server") },
  { id: "Forge.DebugCoreDump", title: "Debug Core Dump…", run: () => openAttach("coreDump") },
  { id: "Forge.Debugger.Rerun", title: "Rerun Debug Session", run: restartSession },
  { id: "Forge.Debugger.StepOverInstruction", title: "Step Over Instruction", run: () => step("stepOverInstruction") },
  { id: "Forge.Debugger.StepIntoInstruction", title: "Step Into Instruction", run: () => step("stepIntoInstruction") },
  {
    id: "Forge.Debugger.AddWatchpoint",
    title: "Add Watchpoint",
    run: () => {
      const v = selectedVariable();
      if (v && !v.watch) return addWatchpoint(v.property);
    },
  },
  {
    id: "Forge.Debugger.JumpToLine",
    title: "Set Execution Point to Cursor",
    run: () => {
      const c = caret();
      const s = pausedSession();
      if (c && s?.adapter) return dapApi.goto(s.id, c.path, c.line).catch((e) => console.warn("goto", e));
    },
  },
];
for (const a of actions) registerCommand({ ...a, category: "Debug" });

/** Open an Attach dialog with the current adapters. */
function openAttach(kind: "process" | "server" | "coreDump"): void {
  void refreshDebugAdapters().then(() => setAttachDialog(kind));
}

registerMainMenuEntries("Run", [action("Forge.AttachToDapServer", "Attach to DAP Ser_ver..."), action("Forge.DebugCoreDump", "Debug Core D_ump...")], {
  after: "XDebugger.AttachToProcess",
});
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

// ---- debug adapters for run configurations ----------------------------------------------------

/** Run configurations whose type an adapter debugs (`registerDebugAdapter({ runConfigurationTypes })`). */
registerDebugLauncher({
  id: "dap",
  canDebug: (cfg) => cfg.type === NATIVE_APP || !!adapterForRunType(cfg.type),
  async launch(cfg) {
    await refreshDebugAdapters();
    const adapter = cfg.type === NATIVE_APP ? debugAdapters().find((a) => a.type === (cfg.options?.adapter || "lldb")) : adapterForRunType(cfg.type);
    if (!adapter) {
      showDebuggerBalloon(t("debug.main.cannotDebugName", { name: cfg.name }), "No debug adapter is available. Choose a debugger in the run configuration.");
      return;
    }
    try {
      const request = adapter.resolve ? await adapter.resolve(cfg) : defaultLaunchRequest(adapter.type, cfg);
      showToolWindow("debug");
      await dapApi.start(request);
    } catch (e) {
      showDebuggerBalloon(t("debug.main.cannotDebugName", { name: cfg.name }), String(e).replace(/^Error: /, ""), 8000);
    }
  },
});

/** "Native Application": run a program, or debug it with a debug adapter (LLDB, GDB, …). */
const NATIVE_APP = "native-app";
registerRunConfigurationType({
  id: NATIVE_APP,
  name: "Native Application",
  description: t("debug.main.runAnExecutableDebugStarts"),
  icon: Cpu,
  color: "var(--syn-type)",
  pluginId: "forge.debugger",
  argumentsLabel: "Program arguments",
  debuggable: true,
  fields: [
    { key: "program", label: t("debug.main.executable"), kind: "path", required: true, placeholder: "build/demo" },
    {
      key: "adapter",
      label: t("debug.main.debugger"),
      kind: "enum",
      options: () => debugAdapters().filter((a) => a.launch).map((a) => ({ value: a.type, label: a.label })),
      default: "lldb",
    },
    { key: "stopOnEntry", label: t("debug.main.stopAtTheBeginningOf"), kind: "bool" },
    { key: "launchOptions", label: t("debug.main.launchOptionsJson"), kind: "text", placeholder: '{ "initCommands": ["settings set target.x86-disassembly-flavor intel"] }' },
  ],
  suggestName: (cfg) => cfg.options?.program?.split(/[\\/]/).pop() || undefined,
  buildCommand: (cfg) => ({ program: cfg.options?.program ?? "", args: [], cwd: cfg.workingDirectory ?? "", env: cfg.env ?? {} }),
});

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
