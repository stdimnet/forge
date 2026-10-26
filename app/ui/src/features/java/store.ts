/**
 * JavaForge state in the UI: the JDKs, the Java setup (JDT LS, java-debug, project JDK,
 * language level, the build) and running installs. Everything is computed in Rust
 * (`commands/java.rs`); this module caches the answers for the pages and widgets.
 */
import { createRoot, createSignal } from "solid-js";
import { javaApi, type BuildModel, type BuildView, type InstallProgress, type Jdk, type JavaStatus, type SyncLine } from "@/ipc/java";
import { isPluginActive } from "@/state/project";

export const active = () => isPluginActive("forge.java");

const state = createRoot(() => {
  const [jdks, setJdks] = createSignal<Jdk[]>([]);
  const [status, setStatus] = createSignal<JavaStatus | null>(null);
  const [build, setBuild] = createSignal<BuildView | null>(null);
  const [install, setInstall] = createSignal<{ id: InstallProgress["id"]; label: string; done: number; total: number | null } | null>(null);
  const [model, setModel] = createSignal<BuildModel | null>(null);
  const [syncing, setSyncing] = createSignal(false);
  const [syncLog, setSyncLog] = createSignal<SyncLine[]>([]);
  const [syncError, setSyncError] = createSignal<string | null>(null);
  return { jdks, setJdks, status, setStatus, build, setBuild, install, setInstall, model, setModel, syncing, setSyncing, syncLog, setSyncLog, syncError, setSyncError };
});

/** The Gradle / Maven tool window's model (projects, tasks, dependencies of the last sync). */
export const buildModel = state.model;
export const syncing = state.syncing;
export const syncLog = state.syncLog;
export const syncError = state.syncError;
export const clearSyncLog = () => {
  state.setSyncLog([]);
  state.setSyncError(null);
};

export async function refreshModel(): Promise<BuildModel | null> {
  const m = await javaApi.buildModel().catch(() => null);
  state.setModel(m);
  return m;
}

let syncListening = false;

/** Reload All Gradle / Maven Projects: the build tool reports tasks and dependency trees. */
export async function syncProject(): Promise<BuildModel | null> {
  if (state.syncing()) return state.model();
  if (!syncListening) {
    syncListening = true;
    void javaApi.onSync((l) => state.setSyncLog((log) => [...log.slice(-499), l]));
  }
  state.setSyncLog([]);
  state.setSyncError(null);
  state.setSyncing(true);
  try {
    const m = await javaApi.syncProject();
    state.setModel(m);
    void refreshBuild();
    return m;
  } catch (e) {
    state.setSyncError(String(e).replace(/^Error: /, ""));
    return null;
  } finally {
    state.setSyncing(false);
  }
}

export const jdks = state.jdks;
export const javaStatus = state.status;
export const buildView = state.build;
export const activeInstall = state.install;

export async function refreshJdks(refresh = false): Promise<Jdk[]> {
  const list = await javaApi.jdks(refresh).catch(() => [] as Jdk[]);
  state.setJdks(list);
  return list;
}

export async function refreshStatus(): Promise<JavaStatus | null> {
  const s = await javaApi.status().catch(() => null);
  state.setStatus(s);
  return s;
}

export async function refreshBuild(): Promise<BuildView | null> {
  const b = await javaApi.buildInfo().catch(() => null);
  state.setBuild(b);
  return b;
}

/** `21 Eclipse Temurin 21.0.4` (IntelliJ's SDK combo box text). */
export const jdkLabel = (j: Jdk) => `${j.name}${j.version ? ` ${j.version}` : ""}${j.vendor ? ` (${j.vendor})` : ""}`;

/** JDK choices of run configurations and settings (value: the home). */
export const jdkChoices = () => state.jdks().map((j) => ({ value: j.home, label: jdkLabel(j) }));

let listening = false;

/** Mirror `java://install` progress into the status bar. */
export function listenForInstalls(): void {
  if (listening) return;
  listening = true;
  void javaApi.onInstall(({ id, event }) => {
    if (event.kind === "progress") state.setInstall({ id, label: event.label, done: event.done, total: event.total ?? null });
    else if (event.kind === "step") state.setInstall({ id, label: event.message, done: 0, total: null });
  });
}

export const clearInstall = () => state.setInstall(null);
