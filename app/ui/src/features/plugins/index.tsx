/**
 * Third-party plugins and Settings Sync:
 * - the plugin host (`host.tsx`) loads enabled plugins in the main window and reloads them on
 *   `plugins://changed` (install, update, uninstall, enable, disable in any window);
 * - actions: Install Plugin from Disk…, Check Plugin Updates, Settings Sync… and Sync
 *   Settings Now (File | Manage IDE Settings);
 * - the Settings Sync page.
 */
import { createResource, createSignal, For, Show } from "solid-js";
import { listen } from "@/ipc";
import { settingsSyncApi, type SyncReport } from "@/ipc/plugins";
import { balloonsFor } from "@/keymap";
import { registerKeymapActions } from "@/keymap/store";
import { registerCommand } from "@/state/commands";
import { getSetting, loadSettings } from "@/state/settings";
import { registerSettingsPage } from "@/features/settingsPages";
import { SchemaPage } from "@/features/settings/SchemaPage";
import { openSettings } from "@/features/settings";
import { isDialogWindow } from "@/shell/window/mode";
import { reloadPlugins } from "./host";
import { checkPluginUpdates, installFromDisk, refreshInstalled } from "./store";
import "./plugins.css";
import { t } from "@/i18n";

export const PLUGINS_CHANGED_EVENT = "plugins://changed";

const showBalloon = balloonsFor("Settings Sync");
const message = (e: unknown) => (e instanceof Error ? e.message : String(e));

void listen(PLUGINS_CHANGED_EVENT, () => {
  void refreshInstalled();
  void loadSettings().catch(() => undefined);
  if (!isDialogWindow()) void reloadPlugins();
});

if (typeof document !== "undefined" && !isDialogWindow()) {
  void refreshInstalled();
  void reloadPlugins();
}

/** Labels of the settings groups (from `settings_sync_status`). */
let groupLabels = new Map<string, string>();
const label = (ids: string[]) => ids.map((id) => groupLabels.get(id) ?? id).join(", ");

const describe = (r: SyncReport) => {
  const parts: string[] = [];
  if (r.pushed.length) parts.push(`sent ${label(r.pushed)}`);
  if (r.pulled.length) parts.push(`received ${label(r.pulled)}`);
  if (r.conflicts.length) parts.push(`${r.conflicts.length} conflict${r.conflicts.length > 1 ? "s" : ""} (newer change kept, the other backed up)`);
  return parts.length ? parts.join("; ") : "Everything is in sync.";
};

const [lastSync, setLastSync] = createSignal<SyncReport | null>(null);
const [syncing, setSyncing] = createSignal(false);

export async function syncNow(quiet = false): Promise<void> {
  if (syncing()) return;
  setSyncing(true);
  try {
    const r = await settingsSyncApi.syncNow();
    setLastSync(r);
    if (!quiet || r.pulled.length || r.conflicts.length) {
      showBalloon("Settings synced", describe(r) + (r.restartNeeded ? " Restart Forge to load the received configuration files." : ""));
    }
  } catch (e) {
    showBalloon("Settings Sync failed", message(e), 8000, undefined, { type: "error" });
  } finally {
    setSyncing(false);
  }
}

function SettingsSyncPage() {
  const [status] = createResource(() =>
    settingsSyncApi.status().then(
      (s) => ((groupLabels = new Map(s.groups)), s),
      () => null,
    ),
  );
  const report = () => lastSync() ?? status()?.lastReport ?? null;
  const labels = () => new Map(status()?.groups ?? []);
  const synced = () => (getSetting<string[]>("settingsSync.groups") ?? []).map((g) => labels().get(g) ?? g);
  return (
    <div>
      <SchemaPage pageId="settingsSync" />
      <div class="sync-status" data-testid="settings-sync-status">
        <div class="sync-status-row">
          <button class="btn-primary set-btn" disabled={syncing()} onClick={() => void syncNow()}>
            {syncing() ? "Syncing…" : "Sync Now"}
          </button>
          <span class="muted">
            <Show when={report()} fallback={"Not synced yet."}>
              {(r) => `Last sync ${new Date(r().atMs).toLocaleString()}: ${describe(r())}`}
            </Show>
          </span>
        </div>
        <div class="set-section-title" style={{ "margin-top": "10px" }}>
          Synced settings
        </div>
        <div class="sync-groups">
          <For each={synced()}>{(g) => <span>{g}</span>}</For>
        </div>
        <p class="muted">
          Each group is synced as a whole: when it changed on both machines, the newer change wins and the other version is
          backed up (in the sync folder, or in Forge's config folder under settings-sync-backups). API keys and passwords stay in
          the system keychain and are never synced. Forge runs no sync service: the folder or repository is yours.
        </p>
      </div>
    </div>
  );
}

registerSettingsPage({
  id: "settingsSync",
  title: t("settings.page.settingsSync"),
  group: "Settings Sync",
  order: 95,
  keywords: ["backup", "sync", "cloud", "dropbox", "git", "export"],
  component: SettingsSyncPage,
});

registerCommand({ id: "Plugins.InstallFromDisk", title: "Install Plugin from Disk…", category: "Plugins", run: () => installFromDisk() });
registerCommand({ id: "Plugins.CheckForUpdates", title: "Check Plugin Updates", category: "Plugins", run: () => void checkPluginUpdates() });
registerCommand({ id: "SettingsSync.Open", title: "Settings Sync…", category: "File", run: () => openSettings("settingsSync") });
registerCommand({ id: "SettingsSync.SyncNow", title: "Sync Settings Now", category: "File", run: () => syncNow() });
registerKeymapActions([
  { id: "Plugins.InstallFromDisk", title: "Install Plugin from Disk…", group: "Plugins" },
  { id: "Plugins.CheckForUpdates", title: "Check Plugin Updates", group: "Plugins" },
  { id: "SettingsSync.Open", title: "Settings Sync…", group: "Main Menu/File/Manage IDE Settings" },
  { id: "SettingsSync.SyncNow", title: "Sync Settings Now", group: "Main Menu/File/Manage IDE Settings" },
]);

// Sync on startup when it is on; later syncs run on demand (Sync Now) or after settings changes.
if (typeof document !== "undefined" && !isDialogWindow()) {
  setTimeout(() => {
    if (getSetting<boolean>("settingsSync.enabled")) void syncNow(true);
  }, 5000);
  let timer: ReturnType<typeof setTimeout> | undefined;
  void listen<{ key: string }>("settings://changed", (c) => {
    if (!getSetting<boolean>("settingsSync.enabled") || c.key.startsWith("settingsSync.")) return;
    clearTimeout(timer);
    timer = setTimeout(() => void syncNow(true), 30_000);
  });
}
