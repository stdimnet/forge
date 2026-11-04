/**
 * Unity, like Rider's Unity support (logic in Rust: `packs/dotnetforge/gamedev`, IPC
 * `unity_*`):
 *
 * - the **Unity** tool window: the connection to the Forge editor bridge, Play / Pause /
 *   Step / Stop, Refresh Assets, Regenerate Project Files and Unity's console (filters,
 *   search, stack traces with links),
 * - Start Unity (the project's editor version from the Unity Hub), Install Editor Bridge…
 *   (copies the bridge package into `Packages/` after the user agreed), scripts opened from
 *   Unity at their line,
 * - the **Unity Tests** run configuration type (batch mode, NUnit results),
 * - Attach to Unity Process… (the editors and their Mono debugger ports; attaching needs a
 *   Mono debug adapter in Settings).
 */
import { createEffect, createMemo, createResource, createRoot, createSignal, For, on, onCleanup, Show } from "solid-js";
import { render } from "solid-js/web";
import {
  Box,
  CircleX,
  FileCog,
  FlaskConical,
  Info,
  Pause,
  Play,
  Plug,
  PlugZap,
  RefreshCw,
  Square,
  StepForward,
  Trash2,
  TriangleAlert,
  Unplug,
} from "lucide-solid";
import { unityApi, type BridgeCommand, type LogEntry, type PlayMode, type UnityProcess, type UnityStatus } from "@/ipc/gamedev";
import { balloonsFor, registerKeymapActions } from "@/keymap";
import { askConfirm } from "@/features/contextMenu/dialogs";
import { openFileAt } from "@/editor/views";
import { registerToolWindow } from "@/features/registry";
import { action, registerMainMenuEntries } from "@/features/mainMenu/model";
import { registerRunConfigurationType } from "@/features/run/types";
import { refreshDebugAdapters } from "@/features/debug/adapters";
import { dapApi } from "@/features/debug/dapApi";
import { isDialogWindow } from "@/shell/window/mode";
import { registerCommand } from "@/state/commands";
import { showToolWindow } from "@/state/layout";
import { isPluginActive, usesFramework } from "@/state/project";
import { getSetting } from "@/state/settings";
import { workspace } from "@/state/workspace";
import { errorText, runCommandLine } from "./shared";

export const UNITY_PLUGIN = "forge.unity";
export const unityActive = () => isPluginActive(UNITY_PLUGIN) && usesFramework("unity");
const showBalloon = balloonsFor("Unity");

// ---- State --------------------------------------------------------------------------------------

const [status, { refetch: refetchStatus, mutate: setStatus }] = createRoot(() =>
  createResource(
    () => (unityActive() ? (workspace()?.root ?? null) : null),
    () => unityApi.status().catch(() => null as UnityStatus | null),
  ),
);
export const unityStatus = status;
export const refreshUnity = () => refetchStatus();

const [playMode, setPlayMode] = createSignal<PlayMode>("edit");
const [compiling, setCompiling] = createSignal(false);
const [entries, setEntries] = createSignal<LogEntry[]>([]);
const [connecting, setConnecting] = createSignal(false);
export const unityConnected = () => !!status()?.connected;
export const unityLog = entries;

function addEntry(e: LogEntry) {
  const max = Math.max(100, getSetting<number>("unity.console.maxEntries", 5000));
  setEntries((list) => {
    const next = [...list, e];
    return next.length > max ? next.slice(next.length - max) : next;
  });
}

/** Connect to the bridge in the editor that has the project open. */
export async function connectUnity(quiet = false): Promise<boolean> {
  if (connecting()) return false;
  setConnecting(true);
  try {
    const w = await unityApi.connect();
    setPlayMode(w.playMode);
    setCompiling(w.compiling);
    setStatus((s) => (s ? { ...s, connected: true, welcome: w, bridgeAvailable: true } : s));
    if (!quiet) showBalloon(`Connected to Unity ${w.unityVersion}`, "", 3000);
    return true;
  } catch (e) {
    if (!quiet) showBalloon("Cannot connect to Unity", errorText(e), 8000, bridgeActions());
    return false;
  } finally {
    setConnecting(false);
  }
}

export async function sendUnity(command: BridgeCommand): Promise<void> {
  if (!unityConnected() && !(await connectUnity())) return;
  try {
    const ack = await unityApi.send(command);
    if (!ack.ok) showBalloon(`Unity could not ${command === "syncSolution" ? "regenerate the project files" : command}`, ack.message ?? "", 8000);
    else if (command === "syncSolution") showBalloon("Project files regenerated", ack.message ?? "", 4000);
  } catch (e) {
    showBalloon(`Unity: ${command}`, errorText(e), 6000);
  }
}

/** Install (or update) the bridge package into `Packages/` after the user agrees. */
export async function installBridge(): Promise<void> {
  const s = status() ?? (await unityApi.status().catch(() => null));
  if (!s?.project) {
    showBalloon("Not a Unity project", "The project has no ProjectSettings/ProjectVersion.txt.", 5000);
    return;
  }
  const update = s.project.bridge.kind !== "none";
  const ok = await askConfirm({
    title: update ? "Update the Forge Editor Bridge" : "Install the Forge Editor Bridge",
    message: `Forge will copy its editor bridge package (dev.forge.unity ${s.bundledBridge}, MIT, C# source) into Packages/dev.forge.unity. Unity compiles it the next time it has focus. The bridge listens on 127.0.0.1 only and lets Forge play, pause and stop the game, refresh assets, show Unity's console and open scripts from Unity.`,
    confirm: update ? "Update" : "Install",
  });
  if (!ok) return;
  try {
    await unityApi.installBridge();
    await refetchStatus();
    showBalloon("Forge Editor Bridge installed", "Switch to Unity to compile it, then choose Forge in Preferences | External Tools | External Script Editor.", 10000);
  } catch (e) {
    showBalloon("Cannot install the editor bridge", errorText(e), 8000);
  }
}

/** Open the project in its Unity editor. */
export async function startUnity(): Promise<void> {
  try {
    const cmd = await unityApi.command("openEditor");
    void runCommandLine(cmd, "Unity Editor");
    setTimeout(() => void refetchStatus(), 4000);
  } catch (e) {
    showBalloon("Cannot start Unity", errorText(e), 8000);
  }
}

/** Regenerate `.sln` / `.csproj`: in the open editor through the bridge, else in batch mode. */
export async function syncSolution(): Promise<void> {
  if (unityConnected()) return sendUnity("syncSolution");
  try {
    const cmd = await unityApi.command("syncSolution");
    const code = await runCommandLine(cmd, "Regenerate Project Files");
    if (code === 0) showBalloon("Project files regenerated", "", 4000);
    void refetchStatus();
  } catch (e) {
    showBalloon("Cannot regenerate the project files", errorText(e), 10000, bridgeActions());
  }
}

function bridgeActions() {
  const s = status();
  if (!s?.project) return [];
  if (s.project.bridge.kind === "none") return [{ label: "Install Editor Bridge…", run: () => void installBridge() }];
  if (!s.editorRunning) return [{ label: "Start Unity", run: () => void startUnity() }];
  return [];
}

// ---- Attach to Unity Process --------------------------------------------------------------------

const [attachOpen, setAttachOpen] = createSignal(false);

async function attach(port: number, host?: string, name?: string): Promise<void> {
  setAttachOpen(false);
  try {
    const request = await unityApi.attachRequest(port, host, name);
    await refreshDebugAdapters();
    showToolWindow("debug");
    await dapApi.start(request);
  } catch (e) {
    showBalloon("Cannot attach to Unity", errorText(e), 12000);
  }
}

function AttachDialog() {
  const [list] = createResource(() => unityApi.processes().catch(() => [] as UnityProcess[]));
  const [host, setHost] = createSignal("127.0.0.1");
  const [port, setPort] = createSignal("");
  const close = () => setAttachOpen(false);
  return (
    <div class="popup-backdrop modal" onMouseDown={(e) => e.target === e.currentTarget && close()}>
      <div class="dialog gd-attach" role="dialog" data-testid="unity-attach">
        <div class="dialog-title">Attach to Unity Process</div>
        <div class="dialog-body">
          <div class="gd-attach-list">
            <For each={list() ?? []} fallback={<div class="muted gd-pad">{list.loading ? "Looking for Unity editors…" : "No Unity editor is running"}</div>}>
              {(p) => (
                <button class="gd-attach-row" onClick={() => void attach(p.debugPort, "127.0.0.1", `Unity Editor (${p.pid})`)}>
                  <Box size={14} />
                  <span class="gd-attach-name">{p.name}</span>
                  <span class="muted">pid {p.pid}</span>
                  <span class="muted gd-attach-project">{p.project ?? ""}</span>
                  <Show when={p.thisProject}>
                    <span class="gd-badge">this project</span>
                  </Show>
                  <span class="mono gd-attach-port">:{p.debugPort}</span>
                </button>
              )}
            </For>
          </div>
          <div class="gd-attach-manual">
            <label class="form-label">Player or other host</label>
            <input class="mono" value={host()} onInput={(e) => setHost(e.currentTarget.value)} />
            <input class="mono gd-port" placeholder="56000" value={port()} onInput={(e) => setPort(e.currentTarget.value)} />
            <button class="btn-secondary" disabled={!/^\d+$/.test(port())} onClick={() => void attach(Number(port()), host())}>
              Attach
            </button>
          </div>
          <p class="muted gd-note">
            Unity's debugger is Mono's soft debugger, not the Debug Adapter Protocol. Attaching needs a Mono debug adapter in Settings | Languages &amp; Frameworks | .NET | Unity, and the editor in Debug mode (Code Optimization On Startup: Debug).
          </p>
        </div>
        <div class="dialog-buttons">
          <button class="btn-secondary" onClick={close}>
            Cancel
          </button>
        </div>
      </div>
    </div>
  );
}

if (typeof document !== "undefined" && !isDialogWindow()) {
  const host = document.createElement("div");
  host.id = "unity-attach-overlay";
  document.body.appendChild(host);
  render(() => (
    <Show when={attachOpen()}>
      <AttachDialog />
    </Show>
  ), host);
}

// ---- Tool window --------------------------------------------------------------------------------

const ICONS = { log: Info, warning: TriangleAlert, error: CircleX, exception: CircleX, assert: CircleX } as const;
const kind = (e: LogEntry) => (e.level === "warning" ? "warning" : e.level === "log" ? "log" : "error");

function UnityToolWindow() {
  const [show, setShow] = createSignal({ error: true, warning: true, log: true });
  const [query, setQuery] = createSignal("");
  const [selected, setSelected] = createSignal<number | null>(null);
  const counts = createMemo(() => {
    const c = { error: 0, warning: 0, log: 0 };
    for (const e of entries()) c[kind(e)]++;
    return c;
  });
  const visible = createMemo(() => {
    const q = query().trim().toLowerCase();
    return entries()
      .map((e, i) => ({ e, i }))
      .filter(({ e }) => show()[kind(e)] && (!q || e.message.toLowerCase().includes(q)));
  });
  const current = () => {
    const i = selected();
    return i == null ? null : (entries()[i] ?? null);
  };
  const open = (e: LogEntry) => e.location && void openFileAt(e.location.path, e.location.line, e.location.column ?? 1);
  const toggle = (k: "error" | "warning" | "log") => setShow((s) => ({ ...s, [k]: !s[k] }));
  const connected = () => unityConnected();
  const s = () => status();
  return (
    <div class="gd-tw" data-testid="unity-window">
      <div class="gd-toolbar">
        <button class="icon-btn" classList={{ active: playMode() !== "edit" }} title="Play" disabled={!connected()} onClick={() => void sendUnity(playMode() === "edit" ? "play" : "stop")} data-testid="unity-play">
          <Play size={14} />
        </button>
        <button class="icon-btn" classList={{ active: playMode() === "paused" }} title="Pause" disabled={!connected()} onClick={() => void sendUnity(playMode() === "paused" ? "resume" : "pause")}>
          <Pause size={14} />
        </button>
        <button class="icon-btn" title="Step" disabled={!connected() || playMode() === "edit"} onClick={() => void sendUnity("step")}>
          <StepForward size={14} />
        </button>
        <button class="icon-btn" title="Stop" disabled={!connected() || playMode() === "edit"} onClick={() => void sendUnity("stop")}>
          <Square size={13} />
        </button>
        <span class="gd-sep" />
        <button class="icon-btn" title="Refresh Assets" disabled={!connected()} onClick={() => void sendUnity("refresh")}>
          <RefreshCw size={14} class={compiling() ? "gd-spin" : undefined} />
        </button>
        <button class="icon-btn" title="Regenerate Project Files" onClick={() => void syncSolution()}>
          <FileCog size={14} />
        </button>
        <span class="gd-sep" />
        <button class="icon-btn" title="Clear" onClick={() => (setEntries([]), setSelected(null))}>
          <Trash2 size={14} />
        </button>
        <button class="gd-filter gd-error" classList={{ off: !show().error }} title="Errors" onClick={() => toggle("error")}>
          <CircleX size={13} /> {counts().error}
        </button>
        <button class="gd-filter gd-warning" classList={{ off: !show().warning }} title="Warnings" onClick={() => toggle("warning")}>
          <TriangleAlert size={13} /> {counts().warning}
        </button>
        <button class="gd-filter" classList={{ off: !show().log }} title="Messages" onClick={() => toggle("log")}>
          <Info size={13} /> {counts().log}
        </button>
        <input class="gd-search" placeholder="Search" value={query()} onInput={(e) => setQuery(e.currentTarget.value)} />
        <span class="gd-spacer" />
        <Show
          when={connected()}
          fallback={
            <button class="gd-conn" onClick={() => void connectUnity()} disabled={connecting()} title="Connect to the Forge editor bridge in Unity">
              <Unplug size={13} /> {connecting() ? "Connecting…" : "Connect"}
            </button>
          }
        >
          <span class="gd-conn on" title={s()?.welcome?.projectPath ?? ""} data-testid="unity-connected">
            <PlugZap size={13} /> Unity {s()?.welcome?.unityVersion}
            <Show when={playMode() !== "edit"}>
              <span class="gd-badge">{playMode() === "paused" ? "Paused" : "Playing"}</span>
            </Show>
            <Show when={compiling()}>
              <span class="gd-badge">Compiling</span>
            </Show>
          </span>
        </Show>
      </div>
      <Show when={entries().length || connected()} fallback={<UnityIntro />}>
        <div class="gd-log" data-testid="unity-log">
          <For each={visible()}>
            {({ e, i }) => {
              const Icon = ICONS[e.level];
              return (
                <div class={`gd-log-row gd-${kind(e)}`} classList={{ selected: selected() === i }} onClick={() => setSelected(i)} onDblClick={() => open(e)}>
                  <Icon size={13} class="gd-log-icon" />
                  <span class="muted gd-log-time">{e.time ?? ""}</span>
                  <span class="gd-log-msg">{e.message.split("\n")[0]}</span>
                </div>
              );
            }}
          </For>
        </div>
        <Show when={current()}>
          {(e) => (
            <div class="gd-detail" data-testid="unity-log-detail">
              <pre class="gd-detail-text">{e().message}</pre>
              <Show when={e().location}>
                {(l) => (
                  <a class="gd-link" onClick={() => open(e())}>
                    {l().path}:{l().line}
                  </a>
                )}
              </Show>
              <Show when={e().stackTrace}>
                <pre class="gd-detail-text muted">{e().stackTrace}</pre>
              </Show>
            </div>
          )}
        </Show>
      </Show>
    </div>
  );
}

function UnityIntro() {
  const s = () => status();
  return (
    <div class="gd-intro" data-testid="unity-intro">
      <Show when={s()?.project} fallback={<div class="muted">Open a Unity project to use this tool window.</div>}>
        {(p) => (
          <>
            <div class="gd-intro-title">
              <Plug size={16} /> Not connected to the Unity editor
            </div>
            <div class="gd-intro-grid">
              <span class="muted">Project version</span>
              <span>Unity {p().version}</span>
              <span class="muted">Editor</span>
              <span>{s()?.editor ? `${s()!.editor!.version}${s()!.exact ? "" : " (closest installed)"} — ${s()!.editor!.path}` : "Not installed: install it with the Unity Hub"}</span>
              <span class="muted">Editor bridge</span>
              <span>{p().bridge.kind === "none" ? "Not installed" : `dev.forge.unity ${p().bridge.version ?? ""}${p().bridge.update ? " (an update is available)" : ""}`}</span>
              <span class="muted">Unity</span>
              <span>{s()?.editorRunning ? `Running (process ${s()!.editorRunning})` : "Not running"}</span>
            </div>
            <div class="gd-intro-actions">
              <Show when={p().bridge.kind === "none" || p().bridge.update}>
                <button class="btn-primary" onClick={() => void installBridge()} data-testid="unity-install-bridge">
                  {p().bridge.kind === "none" ? "Install Editor Bridge…" : "Update Editor Bridge…"}
                </button>
              </Show>
              <Show when={!s()?.editorRunning && s()?.editor}>
                <button class="btn-secondary" onClick={() => void startUnity()}>
                  Start Unity
                </button>
              </Show>
              <Show when={s()?.bridgeAvailable}>
                <button class="btn-secondary" onClick={() => void connectUnity()}>
                  Connect
                </button>
              </Show>
            </div>
            <Show when={p().csprojects.length === 0}>
              <div class="muted gd-note">
                There are no project files yet: C# code insight needs the .sln / .csproj files Unity's IDE packages write. Use Regenerate Project Files, or open the project in Unity with Forge as its external script editor.
              </div>
            </Show>
          </>
        )}
      </Show>
    </div>
  );
}

registerToolWindow({
  id: "unity",
  title: "Unity",
  icon: Box,
  anchor: "bottom",
  order: 48,
  component: UnityToolWindow,
  when: unityActive,
});

// ---- Run configurations -------------------------------------------------------------------------

const opt = (cfg: { options?: Record<string, string> }, key: string) => cfg.options?.[key] ?? "";
const scope = (cfg: { options?: Record<string, string> }) => opt(cfg, "scope") || "all";
const inScope = (...list: string[]) => (cfg: { options?: Record<string, string> }) => list.includes(scope(cfg));

registerRunConfigurationType({
  id: "unity-test",
  name: "Unity Tests",
  description: "Run Unity Test Framework tests (Edit Mode or Play Mode) in batch mode; results from the NUnit report. Unity must not have the project open.",
  icon: FlaskConical,
  color: "var(--success)",
  pluginId: UNITY_PLUGIN,
  argumentsLabel: "Unity options",
  testRunner: true,
  noRunTarget: true,
  fields: [
    {
      key: "scope",
      label: "Test scope",
      kind: "segmented",
      default: "all",
      options: [
        { value: "all", label: "All" },
        { value: "class", label: "Class" },
        { value: "method", label: "Method" },
        { value: "filter", label: "Filter" },
      ],
    },
    { key: "class", label: "Class", required: true, placeholder: "Shooter.Tests.PlayerTests", visible: inScope("class", "method") },
    { key: "method", label: "Method", required: true, placeholder: "Jump_SetsVelocity", visible: inScope("method") },
    { key: "filter", label: "Filter", required: true, placeholder: "a test name or a regular expression", visible: inScope("filter") },
    {
      key: "platform",
      label: "Test platform",
      kind: "enum",
      default: "",
      options: [
        { value: "", label: "Settings | Unity (Edit Mode)" },
        { value: "EditMode", label: "Edit Mode" },
        { value: "PlayMode", label: "Play Mode" },
      ],
    },
    { key: "category", label: "Categories", placeholder: "Fast;Physics" },
    { key: "assemblies", label: "Assemblies", placeholder: "Shooter.Tests.EditMode" },
  ],
  suggestName: (cfg) => {
    switch (scope(cfg)) {
      case "class":
        return opt(cfg, "class").split(".").pop() || undefined;
      case "method":
        return opt(cfg, "method") ? `${opt(cfg, "class").split(".").pop()}.${opt(cfg, "method")}` : undefined;
      case "filter":
        return opt(cfg, "filter") || undefined;
      default:
        return "Unity Tests";
    }
  },
});

// ---- Actions ------------------------------------------------------------------------------------

const needsUnity = (run: () => void) => () => {
  if (!unityActive()) {
    showBalloon("Not a Unity project", "Open a folder with ProjectSettings/ProjectVersion.txt, or enable the Unity plugin.", 5000);
    return;
  }
  run();
};

registerKeymapActions([
  { id: "unity.startEditor", title: "Start Unity", group: "Plugins/Unity" },
  { id: "unity.installBridge", title: "Install Editor Bridge…", group: "Plugins/Unity" },
  { id: "unity.connect", title: "Connect to Unity Editor", group: "Plugins/Unity" },
  { id: "unity.play", title: "Play in Unity", group: "Plugins/Unity" },
  { id: "unity.pause", title: "Pause in Unity", group: "Plugins/Unity" },
  { id: "unity.stop", title: "Stop in Unity", group: "Plugins/Unity" },
  { id: "unity.refresh", title: "Refresh Unity Assets", group: "Plugins/Unity" },
  { id: "unity.syncSolution", title: "Regenerate Project Files", group: "Plugins/Unity" },
  { id: "unity.attach", title: "Attach to Unity Process…", group: "Plugins/Unity" },
]);

registerCommand({ id: "unity.startEditor", title: "Start Unity", category: "Unity", run: needsUnity(() => void startUnity()) });
registerCommand({ id: "unity.installBridge", title: "Install Editor Bridge…", category: "Unity", run: needsUnity(() => void installBridge()) });
registerCommand({ id: "unity.connect", title: "Connect to Unity Editor", category: "Unity", run: needsUnity(() => void connectUnity()) });
registerCommand({ id: "unity.play", title: "Play in Unity", category: "Unity", run: needsUnity(() => void sendUnity("play")) });
registerCommand({ id: "unity.pause", title: "Pause in Unity", category: "Unity", run: needsUnity(() => void sendUnity("pause")) });
registerCommand({ id: "unity.stop", title: "Stop in Unity", category: "Unity", run: needsUnity(() => void sendUnity("stop")) });
registerCommand({ id: "unity.refresh", title: "Refresh Unity Assets", category: "Unity", run: needsUnity(() => void sendUnity("refresh")) });
registerCommand({ id: "unity.syncSolution", title: "Regenerate Project Files", category: "Unity", run: needsUnity(() => void syncSolution()) });
registerCommand({ id: "unity.attach", title: "Attach to Unity Process…", category: "Unity", run: needsUnity(() => setAttachOpen(true)) });

registerMainMenuEntries(
  "_Tools/_Unity",
  [
    action("unity.startEditor", "_Start Unity"),
    action("unity.connect", "_Connect to Unity Editor"),
    action("unity.installBridge", "_Install Editor Bridge…"),
    action("unity.syncSolution", "_Regenerate Project Files"),
    action("unity.refresh", "Re_fresh Assets"),
  ],
  { when: unityActive },
);
registerMainMenuEntries("_Run", [action("unity.attach", "Attach to _Unity Process…")], { when: unityActive });

// ---- Startup: events and the automatic connection ---------------------------------------------------

if (typeof document !== "undefined" && !isDialogWindow()) {
  void unityApi.onBridge((e) => {
    switch (e.type) {
      case "state":
        setPlayMode(e.playMode);
        setCompiling(e.compiling);
        break;
      case "log":
        addEntry(e.entry);
        break;
      case "openFile":
        void openFileAt(e.path, e.line, e.column);
        break;
      case "disconnected":
        setPlayMode("edit");
        setCompiling(false);
        setStatus((s) => (s ? { ...s, connected: false, welcome: null } : s));
        break;
    }
  });
  createRoot(() => {
    createEffect(
      on(
        () => [unityActive(), workspace()?.root] as const,
        ([active]) => {
          setEntries([]);
          if (!active) return;
          // Reconnect while the bridge runs (the editor restarts its listener after every
          // script reload).
          const tick = () => {
            if (!unityActive() || unityConnected() || connecting() || !getSetting<boolean>("unity.bridge.autoConnect", true)) return;
            void unityApi.status().then((s) => {
              setStatus(s);
              if (s.bridgeAvailable && !s.connected) void connectUnity(true);
            });
          };
          const timer = setInterval(tick, 5000);
          setTimeout(tick, 500);
          onCleanup(() => clearInterval(timer));
          void Promise.resolve(refetchStatus()).then((s) => {
            if (s?.project && !s.remote && s.project.bridge.kind === "none")
              showBalloon("Unity project", "Install the Forge editor bridge to play, pause and stop the game, see Unity's console and open scripts from Unity.", 12000, [
                { label: "Install Editor Bridge…", run: () => void installBridge() },
              ]);
          });
        },
      ),
    );
  });
}
