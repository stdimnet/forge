/**
 * Background worker: the only part that talks to Forge (and holds the pairing token).
 * Handles the popup, the content scripts, the context menu and the inspect shortcut.
 */
import { describeBrowser, ForgeClient, type Discovered } from "./lib/forgeClient";
import { firstFileReference } from "./lib/links";
import { displayPath, toOpenTarget } from "./lib/paths";
import {
  DEFAULT_SETTINGS,
  type BackgroundRequest,
  type ContentConfig,
  type ContentMessage,
  type OpenResult,
  type RecentEntry,
  type Settings,
  type SourceInfo,
  type Status,
} from "./lib/types";

const client = new ForgeClient();
const MAX_RECENT = 10;
const ACCENT = "#ff7a1a";
/** How long a successful ping is trusted before asking Forge again. */
const PING_TTL_MS = 4000;

let connection: (Discovered & { at: number }) | null = null;

// ---- Storage -------------------------------------------------------------------------

async function settings(): Promise<Settings> {
  const stored = (await chrome.storage.local.get(null)) as Partial<Settings>;
  return { ...DEFAULT_SETTINGS, ...stored };
}

const save = (patch: Partial<Settings>) => chrome.storage.local.set(patch);

/** Inspect mode per tab; session storage survives the worker being suspended. */
async function inspectState(): Promise<Record<string, boolean>> {
  const area = chrome.storage.session ?? chrome.storage.local;
  return ((await area.get("inspect")).inspect as Record<string, boolean> | undefined) ?? {};
}

async function isInspecting(tabId?: number): Promise<boolean> {
  return tabId !== undefined && !!(await inspectState())[tabId];
}

async function storeInspect(tabId: number, enabled: boolean): Promise<void> {
  const area = chrome.storage.session ?? chrome.storage.local;
  const state = await inspectState();
  if (enabled) state[tabId] = true;
  else delete state[tabId];
  await area.set({ inspect: state });
}

async function setInspect(tabId: number, enabled: boolean): Promise<void> {
  await storeInspect(tabId, enabled);
  await chrome.action.setBadgeBackgroundColor({ tabId, color: ACCENT });
  await chrome.action.setBadgeText({ tabId, text: enabled ? "ON" : "" });
  await chrome.tabs.sendMessage<ContentMessage>(tabId, { type: "inspect", enabled }).catch(() => {});
}

async function activeTabId(): Promise<number | undefined> {
  const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
  return tab?.id;
}

async function inspectShortcut(): Promise<string> {
  const commands = await chrome.commands.getAll().catch(() => []);
  return commands.find((c) => c.name === "toggle-inspect")?.shortcut ?? "";
}

// ---- Forge ---------------------------------------------------------------------------

async function connect(force = false): Promise<Discovered | null> {
  if (!force && connection && Date.now() - connection.at < PING_TTL_MS) return connection;
  const s = await settings();
  const found = await client.discover(s.port, s.token);
  connection = found ? { ...found, at: Date.now() } : null;
  if (found && found.port !== s.port) await save({ port: found.port });
  return found;
}

async function status(tabId?: number): Promise<Status> {
  const s = await settings();
  const conn = await connect(true);
  return {
    connected: !!conn,
    port: conn?.port,
    ping: conn?.ping,
    error: conn ? undefined : "Forge is not running",
    inspecting: await isInspecting(tabId ?? (await activeTabId())),
    // The popup only needs to know whether a token exists.
    settings: { ...s, token: s.token ? "paired" : undefined },
  };
}

async function remember(entry: RecentEntry): Promise<void> {
  const { recent } = await settings();
  const key = (e: RecentEntry) => `${e.file}:${e.line ?? ""}`;
  await save({ recent: [entry, ...recent.filter((e) => key(e) !== key(entry))].slice(0, MAX_RECENT) });
}

const fileName = (p: string) => p.split(/[\\/]/).pop() ?? p;

async function openSource(source: SourceInfo): Promise<OpenResult> {
  const s = await settings();
  if (!s.token) return { ok: false, error: "Not paired with Forge yet. Click the Forge Companion icon to pair." };
  const conn = await connect();
  if (!conn) return { ok: false, error: "Forge is not running. Start Forge and try again." };
  if (!conn.ping.paired) {
    return { ok: false, error: "Forge does not know this browser any more. Pair again from the extension popup." };
  }
  const roots = (conn.ping.projects ?? []).map((p) => p.root);
  try {
    if (source.file) {
      const target = toOpenTarget(
        source.file,
        { roots, mappings: s.mappings },
        { line: source.line, column: source.column, symbol: source.line ? undefined : source.name },
      );
      const outcome = await client.open(conn.port, s.token, target);
      const line = outcome.line ?? target.line;
      const label = `${fileName(outcome.file)}${line ? `:${line}` : ""}`;
      await remember({
        label: source.name ?? fileName(outcome.file),
        file: outcome.file,
        path: displayPath(outcome.file, roots),
        line: line ?? undefined,
        column: target.column,
        at: Date.now(),
      });
      return { ok: true, status: outcome.status, label };
    }
    if (source.name) {
      const outcome = await client.openSymbol(conn.port, s.token, source.name, "component");
      const line = outcome.line ?? undefined;
      await remember({ label: source.name, file: outcome.file, path: displayPath(outcome.file, roots), line, at: Date.now() });
      return { ok: true, status: outcome.status, label: `${displayPath(outcome.file, roots)}${line ? `:${line}` : ""}` };
    }
    return { ok: false, error: "No source location found for this element" };
  } catch (e) {
    connection = null;
    return { ok: false, error: (e as Error).message };
  }
}

async function openText(text: string): Promise<OpenResult> {
  const ref = firstFileReference(text);
  if (!ref) return { ok: false, error: "No file:line reference found in the selection" };
  return openSource({ framework: "attribute", file: ref.file, line: ref.line, column: ref.column });
}

async function pair(code: string): Promise<Status> {
  const found = await client.discover((await settings()).port);
  if (!found) throw new Error("Forge is not running. Start Forge, then pair again.");
  const { token } = await client.pair(found.port, code, describeBrowser(navigator.userAgent));
  await save({ token, port: found.port });
  connection = null;
  return status();
}

// ---- Messages ------------------------------------------------------------------------

async function handle(msg: BackgroundRequest, sender: chrome.runtime.MessageSender): Promise<unknown> {
  const tabId = sender.tab?.id;
  switch (msg.type) {
    case "status":
      return status();
    case "pair":
      return pair(msg.code);
    case "unpair":
      await chrome.storage.local.remove("token");
      connection = null;
      return status();
    case "open":
      return openSource(msg.source);
    case "openText":
      return openText(msg.text);
    case "openRecent":
      return openSource({ framework: "attribute", name: msg.entry.label, file: msg.entry.file, line: msg.entry.line, column: msg.entry.column });
    case "setInspect": {
      const id = msg.tabId ?? tabId ?? (await activeTabId());
      if (id !== undefined) await setInspect(id, msg.enabled);
      return { ok: true };
    }
    case "getInspect":
      return isInspecting(msg.tabId ?? tabId);
    case "contentConfig": {
      const s = await settings();
      const conn = s.token ? await connect().catch(() => null) : null;
      const config: ContentConfig = {
        altClick: s.altClick,
        mappings: s.mappings,
        roots: (conn?.ping.projects ?? []).map((p) => p.root),
        inspecting: await isInspecting(tabId),
        shortcut: await inspectShortcut(),
      };
      return config;
    }
    case "saveOptions": {
      const patch: Partial<Settings> = {};
      if (msg.altClick !== undefined) patch.altClick = msg.altClick;
      if (msg.mappings !== undefined) patch.mappings = msg.mappings;
      if (msg.port !== undefined) patch.port = msg.port ?? undefined;
      await save(patch);
      connection = null;
      return status();
    }
  }
}

chrome.runtime.onMessage.addListener((msg: BackgroundRequest, sender, sendResponse) => {
  handle(msg, sender).then(sendResponse, (e: Error) => sendResponse({ ok: false, error: e.message }));
  return true;
});

// ---- Shortcut, context menu ------------------------------------------------------------

chrome.commands.onCommand.addListener(async (command, tab) => {
  if (command !== "toggle-inspect") return;
  const id = tab?.id ?? (await activeTabId());
  if (id !== undefined) await setInspect(id, !(await isInspecting(id)));
});

chrome.runtime.onInstalled.addListener(() => {
  chrome.contextMenus.removeAll(() => {
    chrome.contextMenus.create({
      id: "forge-open-element",
      title: "Open Component in Forge",
      contexts: ["page", "frame", "link", "image", "video", "audio", "editable"],
    });
    chrome.contextMenus.create({ id: "forge-open-selection", title: "Open “%s” in Forge", contexts: ["selection"] });
  });
});

chrome.contextMenus.onClicked.addListener(async (info, tab) => {
  const tabId = tab?.id;
  const toast = (r: OpenResult) =>
    tabId !== undefined &&
    chrome.tabs
      .sendMessage<ContentMessage>(tabId, {
        type: "toast",
        text: r.ok ? `Opened ${r.label} in Forge` : (r.error ?? "Could not open the file"),
        error: !r.ok,
      })
      .catch(() => {});
  if (info.menuItemId === "forge-open-selection" && info.selectionText) {
    toast(await openText(info.selectionText));
  } else if (info.linkUrl && firstFileReference(info.linkUrl)?.line) {
    // A link to `file:line` (stack trace viewers, error pages) opens that file.
    toast(await openText(info.linkUrl));
  } else if (tabId !== undefined) {
    await chrome.tabs.sendMessage<ContentMessage>(tabId, { type: "openContextTarget" }, { frameId: info.frameId }).catch(() => {});
  }
});

chrome.tabs.onRemoved.addListener((tabId) => void storeInspect(tabId, false));

/** Hooks for automated tests (Playwright drives the service worker directly). */
Object.assign(globalThis, { forgeCompanion: { setInspect, openSource, status } });
