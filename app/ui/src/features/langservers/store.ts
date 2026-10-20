/**
 * Language server rows shared by the settings page, the install dialog and the status
 * bar: refreshed on demand, after installs and when servers change state.
 */
import { createSignal } from "solid-js";
import { listen } from "@/ipc";
import { langserversApi, type InstallLine, type LicenceStatus, type ServerRow } from "@/ipc/langservers";
import { lspApi } from "@/ipc/lsp";
import { SETTINGS_CHANGED_EVENT, type SettingsChange } from "@/ipc/settings";

const [rows, setRows] = createSignal<ServerRow[]>([]);
const [loading, setLoading] = createSignal(false);
const [licence, setLicence] = createSignal<LicenceStatus | null>(null);

export { rows, loading, licence };

let pending: Promise<void> | null = null;

export function refreshRows(): Promise<void> {
  if (pending) return pending;
  setLoading(true);
  const p: Promise<void> = langserversApi
    .list()
    .then((r) => void setRows(r))
    .catch((e) => console.warn("language servers could not be listed", e))
    .finally(() => {
      pending = null;
      setLoading(false);
    });
  pending = p;
  return p;
}

export async function refreshLicence(): Promise<void> {
  setLicence(await langserversApi.licenceStatus().catch(() => null));
}

export async function saveLicence(key: string | null): Promise<LicenceStatus> {
  const s = await langserversApi.setLicence(key);
  setLicence(s);
  return s;
}

/** Output lines per server id, kept while the install dialog is open. */
type OutputListener = (l: InstallLine) => void;
const outputListeners = new Set<OutputListener>();
export function onInstallOutput(cb: OutputListener): () => void {
  outputListeners.add(cb);
  return () => outputListeners.delete(cb);
}

let started = false;
let applyTimer: ReturnType<typeof setTimeout> | undefined;
let statusTimer: ReturnType<typeof setTimeout> | undefined;

/** Subscribe to backend events once. */
export function startLangserverListeners(): void {
  if (started) return;
  started = true;
  void langserversApi.onInstallOutput((l) => outputListeners.forEach((cb) => cb(l)));
  void langserversApi.onChanged(() => void refreshRows());
  // Server state changes (running, indexing) show up on the settings page.
  void lspApi.onStatus(() => {
    clearTimeout(statusTimer);
    statusTimer = setTimeout(() => void refreshRows(), 300);
  });
  // Re-plan when the user changed a language server setting or switched plugins.
  void listen<SettingsChange>(SETTINGS_CHANGED_EVENT, (c) => {
    if (!c.key.startsWith("langservers.") && c.key !== "plugins.disabled") return;
    clearTimeout(applyTimer);
    applyTimer = setTimeout(() => {
      langserversApi
        .apply()
        .then(setRows)
        .catch((e) => console.warn("language servers could not be applied", e));
    }, 200);
  });
}
