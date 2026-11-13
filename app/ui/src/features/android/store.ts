/**
 * Android state in the UI: the SDK and modules, devices and emulators, and the Logcat view.
 * Everything is computed in Rust (`commands/android.rs`, `packs/javaforge/android`): Logcat
 * entries arrive parsed and already filtered by the query; this module keeps what the
 * tool windows render.
 */
import { batch, createRoot, createSignal } from "solid-js";
import { androidApi, type AndroidStatus, type DevicesSnapshot, type LogEntry } from "@/ipc/android";
import { isPluginActive } from "@/state/project";
import { getSetting } from "@/state/settings";

export const PLUGIN_ID = "forge.android";
export const active = () => isPluginActive(PLUGIN_ID);

/** Entries the Logcat view keeps (older ones stay in Rust's buffer). */
const VIEW_LIMIT = 5000;

const state = createRoot(() => {
  const [status, setStatus] = createSignal<AndroidStatus | null>(null);
  const [devices, setDevices] = createSignal<DevicesSnapshot | null>(null);
  const [loadingDevices, setLoadingDevices] = createSignal(false);
  const [serial, setSerial] = createSignal<string | null>(null);
  const [entries, setEntries] = createSignal<LogEntry[]>([]);
  const [query, setQuery] = createSignal<string>("");
  const [paused, setPaused] = createSignal(false);
  const [streaming, setStreaming] = createSignal(false);
  const [counts, setCounts] = createSignal({ total: 0, matched: 0 });
  const [packages, setPackages] = createSignal<string[]>([]);
  const [error, setError] = createSignal<string | null>(null);
  return { status, setStatus, devices, setDevices, loadingDevices, setLoadingDevices, serial, setSerial, entries, setEntries, query, setQuery, paused, setPaused, streaming, setStreaming, counts, setCounts, packages, setPackages, error, setError };
});

export const androidStatus = state.status;
export const devicesSnapshot = state.devices;
export const loadingDevices = state.loadingDevices;
export const logcatSerial = state.serial;
export const logcatEntries = state.entries;
export const logcatQuery = state.query;
export const logcatPaused = state.paused;
export const logcatStreaming = state.streaming;
export const logcatCounts = state.counts;
export const logcatPackages = state.packages;
export const logcatError = state.error;

export async function refreshStatus(): Promise<AndroidStatus | null> {
  const s = await androidApi.status().catch(() => null);
  state.setStatus(s);
  return s;
}

export async function refreshDevices(): Promise<DevicesSnapshot | null> {
  state.setLoadingDevices(true);
  try {
    const d = await androidApi.devices();
    state.setDevices(d);
    return d;
  } catch (e) {
    state.setDevices({ devices: [], avds: [], adb: null, emulator: null, hint: null, error: String(e).replace(/^Error: /, "") });
    return null;
  } finally {
    state.setLoadingDevices(false);
  }
}

/** Online devices, emulators first (the Logcat device list and Android App runs). */
export const onlineDevices = () =>
  (state.devices()?.devices ?? []).filter((d) => d.state === "device").sort((a, b) => Number(b.emulator) - Number(a.emulator));

export const deviceLabel = (serial: string | null) => {
  const d = state.devices()?.devices.find((x) => x.serial === serial);
  if (!d) return serial ?? "";
  return (d.avd ?? d.model ?? d.serial).replace(/_/g, " ");
};

let listening = false;

function listen() {
  if (listening) return;
  listening = true;
  void androidApi.onLogcat((b) => {
    if (b.serial !== state.serial()) return;
    if (b.cleared) {
      batch(() => {
        state.setEntries([]);
        state.setCounts({ total: 0, matched: 0 });
      });
      return;
    }
    if (b.ended) {
      state.setStreaming(false);
      return;
    }
    if (state.paused() || !b.entries.length) return;
    batch(() => {
      const next = [...state.entries(), ...b.entries];
      state.setEntries(next.length > VIEW_LIMIT ? next.slice(next.length - VIEW_LIMIT) : next);
      state.setCounts((c) => ({ total: c.total + b.entries.length, matched: c.matched + b.entries.length }));
    });
  });
}

/** Ask Rust for the entries that match the current query. */
export async function reloadView(): Promise<void> {
  const v = await androidApi.logcatView(state.query(), VIEW_LIMIT).catch(() => null);
  if (!v) return;
  batch(() => {
    state.setEntries(v.entries);
    state.setCounts({ total: v.total, matched: v.matched });
    state.setPackages(v.packages);
  });
}

/** Stream the Logcat of a device. */
export async function startLogcat(serial: string): Promise<void> {
  listen();
  if (!state.query()) state.setQuery(getSetting<string>("android.logcat.defaultQuery", "package:mine"));
  batch(() => {
    state.setSerial(serial);
    state.setEntries([]);
    state.setError(null);
    state.setPaused(false);
  });
  try {
    await androidApi.logcatStart(serial, state.query());
    state.setStreaming(true);
    await reloadView();
  } catch (e) {
    state.setStreaming(false);
    state.setError(String(e).replace(/^Error: /, ""));
  }
}

let timer: ReturnType<typeof setTimeout> | undefined;

/** Change the query; the view is filtered again in Rust shortly after typing stops. */
export function setQuery(q: string): void {
  state.setQuery(q);
  if (timer) clearTimeout(timer);
  timer = setTimeout(() => void reloadView(), 180);
}

export function togglePause(): void {
  const resume = state.paused();
  state.setPaused(!resume);
  if (resume) void reloadView();
}

export async function clearLogcat(): Promise<void> {
  await androidApi.logcatClear().catch(() => undefined);
  batch(() => {
    state.setEntries([]);
    state.setCounts({ total: 0, matched: 0 });
  });
}

export async function stopLogcat(): Promise<void> {
  await androidApi.logcatStop().catch(() => undefined);
  state.setStreaming(false);
}
