/**
 * The plugin host: loads enabled third-party plugins into sandboxed iframes and connects
 * their `forge` API (`runtime.js`) to the contribution points.
 *
 * - Each plugin runs in its own `<iframe sandbox="allow-scripts">` (opaque origin, CSP
 *   `default-src 'none'`): no access to the IDE's DOM, Tauri IPC, storage or network.
 * - Manifest contributions are registered up front (commands, keymap actions, status items,
 *   tool windows, settings pages), so they show before the plugin's code runs; the code is
 *   loaded on its activation events (`onStartup`, `onCommand:`, `onView:`, `onLanguage:`,
 *   `workspaceContains:`).
 * - Calls that leave the sandbox go to Rust (`plugin_host_*`), which checks the granted
 *   permissions; editor access is checked here because the editor lives in the webview.
 */
import { createEffect, createRoot, createSignal, onCleanup, onMount, Show, type JSX } from "solid-js";
import { EditorView } from "@codemirror/view";
import { registerEditorExtension } from "@/editor/extensions";
import { Puzzle } from "lucide-solid";
import { listen } from "@/ipc";
import { pluginHostApi, pluginsApi, type InstalledPlugin } from "@/ipc/plugins";
import { SETTINGS_CHANGED_EVENT, type SettingsChange } from "@/ipc/settings";
import { registerCommand, unregisterCommand } from "@/state/commands";
import { registerKeymapActions } from "@/keymap/store";
import { balloonsFor } from "@/keymap";
import { registerStatusItem } from "@/features/statusItems";
import { registerToolWindow } from "@/features/registry";
import { registerSettingsPage } from "@/features/settingsPages";
import { SchemaPage } from "@/features/settings/SchemaPage";
import { editor } from "@/state/editor";
import { activeView } from "@/editor/views";
import { languageIdFor } from "@/editor/languages";
import { workspace } from "@/state/workspace";
import { workspaceApi } from "@/ipc/workspace";
import { pluginDocument, VIEW_CSP } from "./sandbox";
import { registerIconTheme } from "@/features/iconThemes/state";
import { t } from "@/i18n";

const showBalloon = balloonsFor("Plugins");

interface Instance {
  plugin: InstalledPlugin;
  frame: HTMLIFrameElement;
  ready: Promise<void>;
  commandSeq: number;
  pendingCommands: Map<number, { resolve: () => void; reject: (e: Error) => void }>;
}

const instances = new Map<string, Instance>();
const [loaded, setLoaded] = createSignal<InstalledPlugin[]>([]);
const [statusTexts, setStatusTexts] = createSignal<Record<string, { text: string; tooltip?: string }>>({});
const [toolWindowHtml, setToolWindowHtml] = createSignal<Record<string, string>>({});
const toolWindowFrames = new Map<string, HTMLIFrameElement>();
let registeredCommands: string[] = [];
let iconThemeRegistrations: (() => void)[] = [];

/** Plugins whose contributions are registered (enabled third-party plugins). */
export const loadedPlugins = loaded;
export const isPluginActive = (id: string) => instances.has(id);

const ownerOf = (contributionId: string) =>
  loaded().find((p) => contributionId.startsWith(`${p.manifest.id}.`));

const canEditor = (p: InstalledPlugin, access: "read" | "write") => p.granted.editor?.includes(access) ?? false;

function post(inst: Instance, message: Record<string, unknown>): void {
  inst.frame.contentWindow?.postMessage({ forgeHost: 1, ...message }, "*");
}

function activeEditorInfo() {
  const path = editor.active;
  const view = activeView();
  if (!path || !view) return null;
  const sel = view.state.selection.main;
  return { path, language: languageIdFor(path) ?? null, text: view.state.doc.toString(), selection: { from: sel.from, to: sel.to } };
}

/** Run one `forge.*` call of a plugin. */
async function handleCall(p: InstalledPlugin, method: string, args: unknown[]): Promise<unknown> {
  const id = p.manifest.id;
  const own = (cid: unknown) => {
    if (typeof cid !== "string" || !cid.startsWith(`${id}.`)) throw new Error(`${String(cid)} does not belong to ${id}`);
    return cid;
  };
  switch (method) {
    case "commands.execute": {
      const target = String(args[0]);
      const { runCommand } = await import("@/state/commands");
      await runCommand(target);
      return null;
    }
    case "statusBar.update": {
      const item = (args[1] ?? {}) as { text?: string; tooltip?: string };
      setStatusTexts((s) => ({ ...s, [own(args[0])]: { text: String(item.text ?? ""), tooltip: item.tooltip } }));
      return null;
    }
    case "toolWindows.setHtml":
      setToolWindowHtml((s) => ({ ...s, [own(args[0])]: String(args[1] ?? "") }));
      return null;
    case "toolWindows.postMessage":
      toolWindowFrames.get(own(args[0]))?.contentWindow?.postMessage({ forgeHost: 1, data: args[1] }, "*");
      return null;
    case "editor.getActive":
      if (!canEditor(p, "read")) throw new Error(`plugin \`${id}\` has no permission to read the editor`);
      return activeEditorInfo();
    case "editor.replaceSelection": {
      if (!canEditor(p, "write")) throw new Error(`plugin \`${id}\` has no permission to edit the editor`);
      const view = activeView();
      if (!view) throw new Error("no editor is open");
      view.dispatch(view.state.replaceSelection(String(args[0] ?? "")));
      return null;
    }
    case "window.showMessage": {
      const type = args[1] === "error" ? "error" : args[1] === "warning" ? "warning" : "info";
      showBalloon(p.manifest.name, String(args[0] ?? ""), 5000, undefined, { type });
      return null;
    }
    case "settings.get":
      return pluginHostApi.getSetting(id, String(args[0]));
    case "settings.set":
      return pluginHostApi.setSetting(id, String(args[0]), args[1] ?? null);
    case "workspace.root":
      return workspace()?.root ?? null;
    case "fs.readFile":
      return pluginHostApi.readFile(id, String(args[0]));
    case "fs.writeFile":
      return pluginHostApi.writeFile(id, String(args[0]), String(args[1] ?? ""));
    case "http.fetch":
      return pluginHostApi.fetch(id, args[0] as { url: string });
    case "process.run":
      return pluginHostApi.run(id, String(args[0]), (args[1] as string[]) ?? []);
    default:
      throw new Error(`unknown API ${method}`);
  }
}

window.addEventListener("message", (event) => {
  const m = event.data as { forgePlugin?: number; kind?: string; id?: number; method?: string; args?: unknown[]; error?: string; forgeView?: number; data?: unknown };
  if (!m || typeof m !== "object") return;
  if (m.forgeView === 1) {
    for (const [twId, frame] of toolWindowFrames) {
      if (frame.contentWindow !== event.source) continue;
      const inst = ownerOf(twId) && instances.get(ownerOf(twId)!.manifest.id);
      if (inst) post(inst, { kind: "event", name: `toolWindow:${twId}`, payload: m.data });
    }
    return;
  }
  if (m.forgePlugin !== 1) return;
  const inst = [...instances.values()].find((i) => i.frame.contentWindow === event.source);
  if (!inst) return;
  if (m.kind === "call" && typeof m.id === "number") {
    handleCall(inst.plugin, String(m.method), m.args ?? []).then(
      (value) => post(inst, { kind: "result", id: m.id, value: value ?? null }),
      (e: unknown) => post(inst, { kind: "result", id: m.id, error: e instanceof Error ? e.message : String(e) }),
    );
  } else if (m.kind === "commandDone" && typeof m.id === "number") {
    const p = inst.pendingCommands.get(m.id);
    inst.pendingCommands.delete(m.id);
    if (m.error) p?.reject(new Error(m.error));
    else p?.resolve();
  }
});

/** Load and activate a plugin's code (once). */
export function activate(plugin: InstalledPlugin): Promise<void> {
  const existing = instances.get(plugin.manifest.id);
  if (existing) return existing.ready;
  const frame = document.createElement("iframe");
  frame.setAttribute("sandbox", "allow-scripts");
  frame.setAttribute("aria-hidden", "true");
  frame.dataset.forgePlugin = plugin.manifest.id;
  frame.style.display = "none";
  const inst: Instance = { plugin, frame, ready: Promise.resolve(), commandSeq: 0, pendingCommands: new Map() };
  inst.ready = new Promise<void>((resolve, reject) => {
    const onActivated = (event: MessageEvent) => {
      const m = event.data as { forgePlugin?: number; kind?: string; error?: string };
      if (event.source !== frame.contentWindow || m?.forgePlugin !== 1 || m.kind !== "activated") return;
      window.removeEventListener("message", onActivated);
      if (m.error) {
        showBalloon(t("tools.plugins.failedToStart", { name: plugin.manifest.name }), m.error.split("\n")[0], 8000, undefined, { type: "error" });
        reject(new Error(m.error));
      } else resolve();
    };
    window.addEventListener("message", onActivated);
    pluginsApi.bundle(plugin.manifest.id).then(
      (bundle) => {
        frame.srcdoc = pluginDocument(plugin.manifest, bundle);
        document.body.appendChild(frame);
      },
      (e: unknown) => {
        window.removeEventListener("message", onActivated);
        instances.delete(plugin.manifest.id);
        const msg = e instanceof Error ? e.message : String(e);
        showBalloon(t("tools.plugins.wasNotLoaded", { name: plugin.manifest.name }), msg, 8000, undefined, { type: "warning" });
        reject(new Error(msg));
      },
    );
  });
  inst.ready.catch(() => undefined);
  instances.set(plugin.manifest.id, inst);
  return inst.ready;
}

async function runPluginCommand(plugin: InstalledPlugin, command: string): Promise<void> {
  await activate(plugin);
  const inst = instances.get(plugin.manifest.id);
  if (!inst) return;
  const id = ++inst.commandSeq;
  await new Promise<void>((resolve, reject) => {
    inst.pendingCommands.set(id, { resolve, reject });
    post(inst, { kind: "command", id, command });
  }).catch((e: Error) => showBalloon(plugin.manifest.name, e.message, 6000, undefined, { type: "error" }));
}

function PluginStatusItem(props: { id: string; plugin: InstalledPlugin; command?: string }): JSX.Element {
  const item = () => statusTexts()[props.id];
  return (
    <Show when={item()?.text}>
      <button
        class="status-item"
        data-plugin-status={props.id}
        title={item()?.tooltip ?? props.plugin.manifest.name}
        onClick={() => props.command && void runPluginCommand(props.plugin, props.command)}
      >
        {item()!.text}
      </button>
    </Show>
  );
}

function viewDocument(html: string): string {
  const css = getComputedStyle(document.documentElement);
  const v = (name: string) => css.getPropertyValue(name).trim();
  const font = getComputedStyle(document.body).fontFamily;
  const bridge = `window.forgeView={postMessage:function(d){parent.postMessage({forgeView:1,data:d},"*")},onMessage:function(cb){window.addEventListener("message",function(e){if(e.source===parent&&e.data&&e.data.forgeHost===1)cb(e.data.data)})}};`;
  return `<!doctype html><html><head><meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="${VIEW_CSP}">
<style>:root{color-scheme:light dark;--bg:${v("--bg-panel")};--fg:${v("--fg")};--fg-muted:${v("--fg-muted")};--accent:${v("--accent")};--border:${v("--border")}}
body{margin:8px;background:var(--bg);color:var(--fg);font:13px ${font}}a{color:var(--accent)}</style>
<script>${bridge}</script></head><body>${html}</body></html>`;
}

function PluginToolWindow(props: { id: string; plugin: InstalledPlugin }): JSX.Element {
  let frame!: HTMLIFrameElement;
  onMount(() => {
    toolWindowFrames.set(props.id, frame);
    void activate(props.plugin);
  });
  onCleanup(() => toolWindowFrames.delete(props.id));
  return (
    <div class="plugin-toolwindow">
      <Show when={toolWindowHtml()[props.id] === undefined}>
        <div class="set-empty">{t("tools.plugins.loadingEllipsis", { name: props.plugin.manifest.name })}</div>
      </Show>
      <iframe
        ref={frame}
        class="plugin-toolwindow-frame"
        sandbox="allow-scripts"
        title={props.plugin.manifest.name}
        srcdoc={viewDocument(toolWindowHtml()[props.id] ?? "")}
      />
    </div>
  );
}

const isLoaded = (id: string) => loaded().some((p) => p.manifest.id === id);

/** Register the contributions of one plugin. */
function contribute(p: InstalledPlugin): void {
  const c = p.manifest.contributes;
  for (const cmd of c.commands ?? []) {
    registerCommand({ id: cmd.id, title: cmd.title, category: cmd.category ?? p.manifest.name, run: () => runPluginCommand(p, cmd.id) });
    registeredCommands.push(cmd.id);
  }
  registerKeymapActions(
    (c.commands ?? []).map((cmd) => {
      const keys = (c.keybindings ?? []).filter((k) => k.command === cmd.id);
      const forMac = keys.filter((k) => k.keymap !== "default").map((k) => k.key);
      const forOthers = keys.filter((k) => k.keymap !== "macos").map((k) => k.key);
      return {
        id: cmd.id,
        title: cmd.title,
        group: `Plugins/${p.manifest.name}`,
        shortcuts: { $default: forOthers, macOS: forMac },
      };
    }),
  );
  for (const s of c.statusItems ?? []) {
    registerStatusItem({
      id: s.id,
      align: s.alignment,
      order: 900 + s.order,
      when: () => isLoaded(p.manifest.id),
      component: () => <PluginStatusItem id={s.id} plugin={p} command={s.command} />,
    });
  }
  for (const tw of c.toolWindows ?? []) {
    registerToolWindow({
      id: tw.id,
      title: tw.title,
      icon: Puzzle,
      anchor: tw.anchor,
      order: 900 + tw.order,
      when: () => isLoaded(p.manifest.id),
      component: () => <PluginToolWindow id={tw.id} plugin={p} />,
    });
  }
  for (const page of c.settingsPages ?? []) {
    registerSettingsPage({
      id: page.id,
      title: page.title,
      group: "Tools",
      order: 900,
      keywords: [p.manifest.name, "plugin"],
      component: () => (
        <Show when={isLoaded(p.manifest.id)} fallback={<div class="set-empty">{t("tools.plugins.thePluginIsDisabled", { name: p.manifest.name })}</div>}>
          <SchemaPage pageId={page.id} />
        </Show>
      ),
    });
  }
}

/** Whether `workspaceContains:<file>` holds for the open project. */
async function projectContains(file: string): Promise<boolean> {
  const root = workspace()?.root;
  if (!root) return false;
  return workspaceApi.readFile(`${root.replace(/\/$/, "")}/${file}`).then(
    () => true,
    () => false,
  );
}

async function activateOnEvents(p: InstalledPlugin): Promise<void> {
  for (const event of p.manifest.activationEvents) {
    if (event === "onStartup") return void activate(p).catch(() => undefined);
    if (event.startsWith("workspaceContains:") && (await projectContains(event.slice(18)))) {
      return void activate(p).catch(() => undefined);
    }
  }
}

/** (Re)load the installed plugins: called at startup and after install, uninstall or enablement changes. */
/** Icon themes are data: they come from every enabled plugin, with or without code. */
function registerIconThemes(installed: InstalledPlugin[]): void {
  for (const remove of iconThemeRegistrations) remove();
  iconThemeRegistrations = installed
    .filter((p) => p.enabled)
    .flatMap((p) =>
      (p.manifest.contributes.iconThemes ?? []).map((th) =>
        registerIconTheme({ id: th.id, label: th.label, source: p.manifest.name, load: () => pluginsApi.iconTheme(th.id) }),
      ),
    );
}

/** Only the icon themes of the installed plugins (dialog windows, which run no plugin code). */
export async function reloadIconThemes(): Promise<void> {
  registerIconThemes(await pluginsApi.installed().catch(() => []));
}

export async function reloadPlugins(): Promise<void> {
  for (const inst of instances.values()) inst.frame.remove();
  instances.clear();
  for (const id of registeredCommands) unregisterCommand(id);
  registeredCommands = [];
  let installed: InstalledPlugin[] = [];
  try {
    installed = await pluginsApi.installed();
  } catch (e) {
    console.warn("installed plugins could not be read", e);
  }
  registerIconThemes(installed);
  const enabled = installed.filter((p) => p.enabled && p.manifest.main);
  setLoaded(enabled);
  setStatusTexts({});
  setToolWindowHtml({});
  for (const p of enabled) contribute(p);
  for (const p of enabled) void activateOnEvents(p);
}

// Activation by language, editor change events and setting change events.
createRoot(() => {
  createEffect(() => {
    const path = editor.active;
    const lang = path ? languageIdFor(path) : undefined;
    for (const p of loaded()) {
      if (lang && p.manifest.activationEvents.includes(`onLanguage:${lang}`)) void activate(p).catch(() => undefined);
    }
  });
});

let changeTimer: ReturnType<typeof setTimeout> | undefined;
/** Tell plugins that may read the editor that the active editor or its text changed. */
export function notifyEditorChanged(): void {
  clearTimeout(changeTimer);
  changeTimer = setTimeout(() => {
    const info = activeEditorInfo();
    for (const inst of instances.values()) {
      if (canEditor(inst.plugin, "read")) post(inst, { kind: "event", name: "editor.changed", payload: info });
    }
  }, 150);
}

createRoot(() => {
  createEffect(() => {
    void editor.active;
    if (instances.size) notifyEditorChanged();
  });
});

registerEditorExtension(() =>
  EditorView.updateListener.of((u) => {
    if (u.docChanged && instances.size) notifyEditorChanged();
  }),
);

void listen<SettingsChange>(SETTINGS_CHANGED_EVENT, (c) => {
  const owner = ownerOf(c.key);
  const inst = owner && instances.get(owner.manifest.id);
  if (inst) post(inst, { kind: "event", name: "settings.changed", payload: { key: c.key, value: c.value } });
});
