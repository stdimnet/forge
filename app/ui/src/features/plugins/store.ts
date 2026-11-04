/**
 * Installing, updating and uninstalling third-party plugins from the UI: the permission
 * prompt, Install Plugin from Disk and the marketplace install. Validation, verification
 * and the install itself happen in Rust (`crates/forge-plugins`).
 */
import { createSignal } from "solid-js";
import { isTauri } from "@/ipc";
import { pluginsApi, type InstalledPlugin, type MarketplaceEntry, type PermissionLine } from "@/ipc/plugins";
import { balloonsFor } from "@/keymap";
import { loadSettings } from "@/state/settings";
import { reloadPlugins } from "./host";

const showBalloon = balloonsFor("Plugins");
const message = (e: unknown) => (e instanceof Error ? e.message : String(e));

/** The permission prompt shown before a plugin is installed or updated. */
export interface PermissionPrompt {
  name: string;
  version: string;
  publisher: string;
  description: string;
  /** Where it comes from: a file path or the marketplace. */
  origin: string;
  lines: PermissionLine[];
  /** Kinds of lines an update adds. */
  added: string[];
  update: boolean;
  accept: () => Promise<void>;
}

const [prompt, setPrompt] = createSignal<PermissionPrompt | null>(null);
const [busy, setBusy] = createSignal<string | null>(null);
const [installed, setInstalled] = createSignal<InstalledPlugin[]>([]);
export { prompt, busy, installed };

export const closePrompt = () => setPrompt(null);

export async function refreshInstalled(): Promise<void> {
  try {
    setInstalled(await pluginsApi.installed());
  } catch {
    setInstalled([]);
  }
}

async function afterChange(): Promise<void> {
  await Promise.all([refreshInstalled(), loadSettings().catch(() => undefined)]);
  await reloadPlugins();
}

/** Install Plugin from Disk: pick a `.forgeplugin` file, then ask for its permissions. */
export async function installFromDisk(path?: string): Promise<void> {
  let file = path;
  if (!file) {
    if (!isTauri()) file = "word-count-1.1.0.forgeplugin";
    else {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const picked = await open({ multiple: false, title: "Install Plugin from Disk", filters: [{ name: "Forge plugin", extensions: ["forgeplugin"] }] });
      if (typeof picked !== "string") return;
      file = picked;
    }
  }
  try {
    const info = await pluginsApi.inspect(file);
    const m = info.manifest;
    setPrompt({
      name: m.name,
      version: m.version,
      publisher: m.publisher,
      description: m.description,
      origin: file,
      lines: info.permissionLines,
      added: info.newPermissionLines.map((l) => l.kind),
      update: !!info.installedVersion,
      accept: async () => {
        await pluginsApi.installFile(file, info.sha256);
        showBalloon(`${m.name} ${m.version} installed`, "The plugin is active now.");
        await afterChange();
      },
    });
  } catch (e) {
    showBalloon("Plugin was not installed", message(e), 8000, undefined, { type: "error" });
  }
}

/** Install or update a marketplace plugin after the permission prompt. */
export function installFromMarketplace(entry: MarketplaceEntry, onDone?: () => void): void {
  const current = installed().find((p) => p.manifest.id === entry.id);
  const had = new Set(current?.permissionLines.map((l) => `${l.kind}:${l.detail}`) ?? []);
  setPrompt({
    name: entry.name,
    version: entry.latest.version,
    publisher: entry.publisher,
    description: entry.description,
    origin: "Marketplace",
    lines: entry.permissionLines,
    added: current ? entry.permissionLines.filter((l) => !had.has(`${l.kind}:${l.detail}`)).map((l) => l.kind) : entry.permissionLines.map((l) => l.kind),
    update: !!current,
    accept: async () => {
      setBusy(entry.id);
      try {
        await pluginsApi.marketplaceInstall(entry.id, entry.latest.version);
        showBalloon(`${entry.name} ${entry.latest.version} installed`, "Checksum verified. The plugin is active now.");
        await afterChange();
        onDone?.();
      } finally {
        setBusy(null);
      }
    },
  });
}

export async function acceptPrompt(): Promise<void> {
  const p = prompt();
  if (!p) return;
  setPrompt(null);
  try {
    await p.accept();
  } catch (e) {
    showBalloon(`${p.name} was not installed`, message(e), 8000, undefined, { type: "error" });
  }
}

export async function uninstall(id: string, name: string): Promise<void> {
  try {
    await pluginsApi.uninstall(id);
    showBalloon(`${name} uninstalled`, "Its files and data were removed.");
    await afterChange();
  } catch (e) {
    showBalloon("Uninstall failed", message(e), 6000, undefined, { type: "error" });
  }
}

/** Help | Check for Updates for plugins: a balloon when the marketplace has newer versions. */
export async function checkPluginUpdates(quiet = false): Promise<MarketplaceEntry[]> {
  try {
    const updates = await pluginsApi.updates();
    if (updates.length) {
      showBalloon(
        "Plugin updates available",
        updates.map((u) => `${u.name} ${u.installedVersion} → ${u.latest.version}`).join(", "),
        8000,
      );
    } else if (!quiet) showBalloon("Plugins are up to date", "");
    return updates;
  } catch (e) {
    if (!quiet) showBalloon("Plugin update check failed", message(e), 6000, undefined, { type: "warning" });
    return [];
  }
}
