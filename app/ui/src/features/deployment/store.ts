/**
 * Deployment state on the UI side: the project's servers, the File Transfer log and the
 * actions of Tools | Deployment (upload, download, sync, compare, delete). The work happens
 * in Rust (`forge-deploy`); this file picks the paths and the server and shows the result.
 */
import { createSignal } from "solid-js";
import { createStore, produce } from "solid-js/store";
import { deploymentApi, type DeployConfig, type Direction, type FileResult, type ServerView, type Summary, type TransferEvent } from "@/ipc/deployment";
import { activeTab } from "@/state/editor";
import { showToolWindow } from "@/state/layout";
import { getSetting, saveSetting } from "@/state/settings";
import { showBalloon } from "@/keymap";
import { askConfirm } from "@/features/contextMenu/dialogs";
import { projectTree, refreshDirs } from "@/features/project/ProjectView";
import { reloadOpenFiles } from "@/features/git/store";
import { setDiffRequest } from "@/features/git/overlays";
import { t } from "@/i18n";

const [config, setConfig] = createSignal<DeployConfig | null>(null);
export { config };

export async function refreshConfig(): Promise<DeployConfig | null> {
  try {
    const c = await deploymentApi.config();
    setConfig(c);
    return c;
  } catch {
    setConfig(null);
    return null;
  }
}

export const defaultServer = (): ServerView | undefined => {
  const c = config();
  return c?.servers.find((s) => s.id === c.defaultServer);
};

// ---- File Transfer log ----------------------------------------------------------------

export interface Operation {
  id: string;
  operation: string;
  server: string;
  paths: string[];
  automatic: boolean;
  started: number;
  files: FileResult[];
  summary?: Summary;
  error?: string | null;
  done: boolean;
}

const [log, setLog] = createStore<{ operations: Operation[] }>({ operations: [] });
export { log };

/** At most this many operations stay in the File Transfer window. */
const MAX_OPERATIONS = 50;

export function applyTransferEvent(e: TransferEvent): void {
  if (e.kind === "started") {
    setLog(
      produce((s) => {
        s.operations.push({
          id: e.id,
          operation: e.operation,
          server: e.server,
          paths: e.paths,
          automatic: e.automatic,
          started: Date.now(),
          files: [],
          done: false,
        });
        if (s.operations.length > MAX_OPERATIONS) s.operations.splice(0, s.operations.length - MAX_OPERATIONS);
      }),
    );
    return;
  }
  const at = log.operations.findIndex((o) => o.id === e.id);
  if (at < 0) return;
  if (e.kind === "file") setLog("operations", at, "files", (f) => [...f, e.file]);
  else setLog("operations", at, { summary: e.summary, error: e.error, done: true });
}

export function clearLog(): void {
  setLog("operations", (ops) => ops.filter((o) => !o.done));
}

let listening = false;
export function listenTransfers(): void {
  if (listening) return;
  listening = true;
  void deploymentApi.onTransfer(applyTransferEvent);
}

let seq = 0;
const newId = () => `t${Date.now().toString(36)}-${++seq}`;

/** Running operation ids (for Cancel in the File Transfer window). */
export const cancelOperation = (id: string) => void deploymentApi.cancel(id);

// ---- Targets ----------------------------------------------------------------------------

/** What the actions work on: the Project view selection when it has the focus, else the open file. */
export function targetPaths(): string[] {
  const inTree = document.activeElement?.closest(".project-tree");
  const selection = projectTree.selectedPaths();
  if (inTree && selection.length) return selection;
  const tab = activeTab();
  if (tab && !tab.path.includes("://") && !tab.path.startsWith("forge:")) return [tab.path];
  return selection.length ? selection : [""];
}

const label = (paths: string[]) => (paths.length === 1 ? paths[0] || "the project" : `${paths.length} items`);

async function ensureServers(): Promise<DeployConfig | null> {
  const c = (await refreshConfig()) ?? config();
  if (!c?.servers.length) {
    showBalloon(t("tools.deployment.noDeploymentServers2"), t("tools.deployment.addAServerInSettings"), 4000, [
      { label: "Configure…", run: () => openDeploymentSettings() },
    ]);
    return null;
  }
  if (!c.available) {
    showBalloon(t("tools.deployment.deploymentIsNotAvailable"), t("tools.deployment.deploymentWorksWithProjectsOn"));
    return null;
  }
  return c;
}

let openSettingsPage: (page: string) => void = () => undefined;
export function setSettingsOpener(open: (page: string) => void): void {
  openSettingsPage = open;
}
export const openDeploymentSettings = () => openSettingsPage("build.deployment");
export const openDeploymentOptions = () => openSettingsPage("build.deployment.options");

// ---- Choose server popup ------------------------------------------------------------------

export interface ServerChoice {
  title: string;
  servers: ServerView[];
  defaultId: string | null;
  pick: (server: ServerView | null) => void;
}

const [choice, setChoice] = createSignal<ServerChoice | null>(null);
export { choice, setChoice };

/** Ask which server to use ("Upload to…"); `null` when cancelled. */
export async function chooseServer(title: string): Promise<ServerView | null> {
  const c = await ensureServers();
  if (!c) return null;
  return new Promise((resolve) =>
    setChoice({
      title,
      servers: c.servers,
      defaultId: c.defaultServer,
      pick: (s) => {
        setChoice(null);
        resolve(s);
      },
    }),
  );
}

async function defaultOrAsk(title: string): Promise<ServerView | null> {
  const c = await ensureServers();
  if (!c) return null;
  const d = c.servers.find((s) => s.id === c.defaultServer);
  return d ?? chooseServer(title);
}

// ---- Actions ------------------------------------------------------------------------------

function report(summary: Summary, what: string, server: string): void {
  if (summary.cancelled) return void showBalloon(`${what} cancelled`);
  if (summary.failed) {
    const first = summary.results.find((r) => r.status === "failed");
    showBalloon(t("tools.deployment.failedForItems", { count: summary.failed, what, server }), first?.message, 5000, [
      { label: "Show File Transfer", run: () => showToolWindow("fileTransfer") },
    ]);
    return;
  }
  const n = summary.transferred + summary.deleted;
  showBalloon(
    `${what} ${server}: ${n === 0 ? "nothing to transfer" : `${n} file${n === 1 ? "" : "s"} ${summary.deleted && !summary.transferred ? "deleted" : "transferred"}`}`,
    summary.skipped ? t("tools.deployment.skippedUpToDateExcluded", { skipped: summary.skipped }) : undefined,
  );
}

/** Upload or download, asking before overwriting newer files. */
export async function transfer(server: ServerView, direction: Direction, paths: string[]): Promise<Summary | null> {
  const what = direction === "upload" ? "Upload to" : "Download from";
  showToolWindow("fileTransfer");
  try {
    let summary = await deploymentApi.transfer(newId(), server.id, direction, paths);
    if (summary.newerOnTarget.length) {
      const n = summary.newerOnTarget.length;
      const target = direction === "upload" ? `on ${server.name}` : "in the project";
      const ok = await askConfirm({
        title: direction === "upload" ? "Upload Over Newer Files" : "Download Over Newer Files",
        message: `${n === 1 ? summary.newerOnTarget[0] : `${n} files`} ${n === 1 ? "is" : "are"} newer ${target} than the source. Overwrite ${n === 1 ? "it" : "them"}?`,
        confirm: "Overwrite",
      });
      if (ok) summary = await deploymentApi.transfer(newId(), server.id, direction, summary.newerOnTarget, true);
    }
    if (direction === "download") {
      refreshDirs([...new Set(paths.map((p) => p.split("/").slice(0, -1).join("/"))), ...paths]);
      await reloadOpenFiles();
    }
    report(summary, what, server.name);
    return summary;
  } catch (e) {
    showBalloon(`${what} ${server.name} failed`, String(e instanceof Error ? e.message : e), 5000);
    return null;
  }
}

export async function uploadToDefault(paths = targetPaths()): Promise<void> {
  const s = await defaultOrAsk("Upload to");
  if (s) await transfer(s, "upload", paths);
}

export async function uploadTo(paths = targetPaths()): Promise<void> {
  const s = await chooseServer("Upload to");
  if (s) await transfer(s, "upload", paths);
}

export async function downloadFromDefault(paths = targetPaths()): Promise<void> {
  const s = await defaultOrAsk("Download from");
  if (s) await transfer(s, "download", paths);
}

export async function downloadFrom(paths = targetPaths()): Promise<void> {
  const s = await chooseServer("Download from");
  if (s) await transfer(s, "download", paths);
}

export async function deleteOnServer(paths = targetPaths(), server?: ServerView | null): Promise<void> {
  const s = server ?? (await defaultOrAsk("Delete from"));
  if (!s) return;
  const ok = await askConfirm({
    title: "Delete on Server",
    message: `Delete ${label(paths)} from ${s.name}? This cannot be undone.`,
    confirm: "Delete",
  });
  if (!ok) return;
  showToolWindow("fileTransfer");
  try {
    report(await deploymentApi.delete(newId(), s.id, paths), "Delete from", s.name);
  } catch (e) {
    showBalloon(t("tools.deployment.deleteFromFailed", { name: s.name }), String(e instanceof Error ? e.message : e), 5000);
  }
}

/** Compare with Deployed Version: the deployed file on the left, the project file on the right. */
export async function compareWithDeployed(path: string, server?: ServerView | null): Promise<void> {
  const s = server ?? (await defaultOrAsk("Compare with Deployed Version on"));
  if (!s) return;
  try {
    const [remote, local] = await Promise.all([deploymentApi.readRemote(s.id, path), readLocal(path)]);
    if (remote.binary) return void showBalloon(t("tools.deployment.cannotCompareBinaryFiles"), remote.remote);
    setDiffRequest({
      path,
      left: { title: remote.text === null ? `${remote.server}: ${remote.remote} (does not exist)` : `${remote.server}: ${remote.remote}`, content: remote.text ?? "" },
      right: { title: `Local: ${path}`, content: local ?? "" },
      editable: local !== null,
    });
  } catch (e) {
    showBalloon(t("tools.deployment.compareWithDeployedVersionFailed"), String(e instanceof Error ? e.message : e), 5000);
  }
}

async function readLocal(path: string): Promise<string | null> {
  const { readText } = await import("@/features/compare/state");
  return readText(path);
}

let openSync: (server: ServerView, path: string) => void = () => undefined;
export function setSyncOpener(open: (server: ServerView, path: string) => void): void {
  openSync = open;
}

/** Sync with Deployed: the folder diff of a project folder and its deployed copy. */
export async function syncWith(server: ServerView | null, path: string): Promise<void> {
  const s = server ?? (await defaultOrAsk("Sync with Deployed to"));
  if (s) openSync(s, path);
}

/** The folder Sync works on: the selected folder, or the folder of the selected file. */
export function syncFolder(): string {
  const [first] = targetPaths();
  const rows = [...document.querySelectorAll<HTMLElement>(".project-tree .tree-row[data-path]")];
  const isDir = rows.find((r) => r.dataset.path === first)?.dataset.dir !== undefined;
  if (first === "" || isDir) return first;
  return first.split("/").slice(0, -1).join("/");
}

// ---- Automatic upload ---------------------------------------------------------------------

export const autoUploadMode = () => getSetting<string>("deployment.autoUpload", "never");
export const autoUploadAlways = () => autoUploadMode() === "always";

/** Tools | Deployment | Automatic Upload (Always): Always ↔ Never for this project. */
export async function toggleAutoUpload(): Promise<void> {
  const next = autoUploadAlways() ? "never" : "always";
  await saveSetting("deployment.autoUpload", next, "project");
  showBalloon(next === "always" ? t("tools.deployment.automaticUploadToIsOn", { value: defaultServer()?.name ?? "the default server" }) : t("tools.deployment.automaticUploadIsOff"));
}

const pending = new Map<string, boolean>();
let timer: ReturnType<typeof setTimeout> | undefined;

/** A file was saved: upload it (with others saved at the same time) when the options say so. */
export function savedForAutoUpload(path: string, explicit: boolean): void {
  if (autoUploadMode() === "never" || path.includes("://") || path.startsWith("forge:")) return;
  pending.set(path, (pending.get(path) ?? false) || explicit);
  clearTimeout(timer);
  timer = setTimeout(() => {
    const paths = [...pending.keys()];
    const anyExplicit = [...pending.values()].some(Boolean);
    pending.clear();
    void deploymentApi.autoUpload(newId(), paths, anyExplicit).then(
      (summary) => {
        if (summary?.failed) report(summary, "Automatic upload to", defaultServer()?.name ?? "the default server");
      },
      (e) => showBalloon(t("tools.deployment.automaticUploadFailed"), String(e instanceof Error ? e.message : e), 5000),
    );
  }, 300);
}
