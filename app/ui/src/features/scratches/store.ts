/**
 * Scratches and Consoles: the list of scratch files (computed and stored by Rust,
 * `forge-scratches`), opening them as editor tabs at `scratch://<name>` and running them.
 */
import { createSignal } from "solid-js";
import { scratchesApi, type ScratchFile } from "@/ipc/scratches";
import { settingsApi } from "@/ipc/settings";
import { workspaceApi } from "@/ipc/workspace";
import { getSetting, scopeFor } from "@/state/settings";
import { showBalloon } from "@/keymap";
import { closeTab, editor, isDirty, openFile, saveFile } from "@/state/editor";
import { activeView } from "@/editor/views";
import { launch } from "@/features/run/runStore";
import { openFileConsole } from "@/features/database/store";
import { confirmDialog } from "@/features/browserCompanion/ConfirmDialog";

export const SCRATCH_PREFIX = "scratch://";
export const scratchPath = (name: string) => `${SCRATCH_PREFIX}${name}`;
export const scratchName = (path: string) => (path.startsWith(SCRATCH_PREFIX) ? path.slice(SCRATCH_PREFIX.length) : null);
export const isScratchPath = (path: string | null | undefined) => !!path?.startsWith(SCRATCH_PREFIX);
/** `.http` request files: scratches and project files. */
export const isHttpPath = (path: string | null | undefined) => !!path && /\.http$/i.test(path);

/** Project setting with the environment `.http` requests run with ("" = the default one). */
export const HTTP_ENV_SETTING = "httpClient.environment";
/** Stored for "No Environment" (environment names starting with `$` are not environments). */
export const NO_HTTP_ENV = "$none";
export const httpEnvironment = () => getSetting<string>(HTTP_ENV_SETTING, "");

export async function setHttpEnvironment(name: string): Promise<void> {
  try {
    await settingsApi.set(HTTP_ENV_SETTING, name || null, scopeFor(HTTP_ENV_SETTING));
  } catch (err) {
    showBalloon("Cannot select the environment", String(err));
  }
}

const [scratches, setScratches] = createSignal<ScratchFile[]>([]);
export { scratches };

/** Languages used for new scratches, most recent first (listed first in the popup). */
const [recentLanguages, setRecentLanguages] = createSignal<string[]>([]);
export { recentLanguages };

export async function refreshScratches(): Promise<void> {
  try {
    setScratches(await scratchesApi.list());
  } catch (err) {
    console.warn("cannot list scratches", err);
  }
}

export const scratchByName = (name: string) => scratches().find((s) => s.name === name);

/** Open a scratch: SQL scratches with a data source open as a query console of it. */
export async function openScratch(file: ScratchFile): Promise<void> {
  const path = scratchPath(file.name);
  if (file.runner === "sql" && file.dataSource && !editor.tabs.some((t) => t.path === path)) {
    const content = await scratchesApi.read(file.name);
    openFileConsole({ path, name: file.name, content }, file.dataSource);
    return;
  }
  await openFile(path);
}

export async function createScratch(language: string): Promise<void> {
  try {
    const file = await scratchesApi.create(language);
    setRecentLanguages((prev) => [language, ...prev.filter((l) => l !== language)].slice(0, 5));
    await refreshScratches();
    await openScratch(file);
  } catch (err) {
    showBalloon("Cannot create scratch file", String(err));
  }
}

export async function createBuffer(): Promise<void> {
  try {
    const file = await scratchesApi.createBuffer();
    await refreshScratches();
    await openScratch(file);
  } catch (err) {
    showBalloon("Cannot create scratch buffer", String(err));
  }
}

/** Current text of a scratch: the open editor's (also unsaved), else the file's. */
export async function scratchContent(name: string): Promise<string> {
  const tab = editor.tabs.find((t) => t.path === scratchPath(name));
  return tab ? tab.content : scratchesApi.read(name);
}

/** Current text of a project file: the open editor's (also unsaved), else the file's. */
export async function fileContent(path: string): Promise<string> {
  const tab = editor.tabs.find((t) => t.path === path);
  return tab ? tab.content : workspaceApi.readFile(path);
}

/** Send the request of an `.http` file at `line` (default: the caret's) in the Run tool window. */
async function runHttp(path: string, content: string, line: number | undefined, options: Record<string, string>): Promise<void> {
  const name = path.split("/").pop() ?? path;
  const at = line ?? caretLine(path);
  const markers = (await scratchesApi.markers(name, content)).filter((m) => m.kind === "http");
  let marker = markers[0];
  for (const m of markers) if (m.line <= at) marker = m;
  if (!marker) return void showBalloon(`No request in '${name}'`, "Add a request such as GET https://example.com");
  await launch({ name: marker.runName, type: "http-request", options: { ...options, line: String(marker.line) } });
}

/** Run a request of a project `.http` file (gutter ▶, Ctrl+Shift+F10). */
export async function runHttpFile(path: string, line?: number): Promise<void> {
  await runHttp(path, await fileContent(path), line, { file: path });
}

/** Run a scratch in the Run tool window (`line` picks the request of an `.http` file). */
export async function runScratch(name: string, line?: number): Promise<void> {
  const file = scratchByName(name) ?? (await scratchesApi.list().then((l) => l.find((s) => s.name === name)));
  const runner = file?.runner;
  if (!runner) return void showBalloon(`'${name}' cannot be run`, "Scratches of this language have no runner.");
  if (runner === "sql") return attachOrOpenConsole(name);
  if (runner === "http") return runHttp(scratchPath(name), await scratchContent(name), line, { scratch: name });
  await launch({ name, type: "scratch", options: { scratch: name } });
}

function caretLine(path: string): number {
  const view = editor.active === path ? activeView() : undefined;
  return view ? view.state.doc.lineAt(view.state.selection.main.head).number - 1 : 0;
}

/** A SQL scratch runs in the console of its data source; without one, ask to attach one. */
async function attachOrOpenConsole(name: string): Promise<void> {
  const file = scratchByName(name);
  if (file?.dataSource) {
    const path = scratchPath(name);
    const tab = editor.tabs.find((t) => t.path === path);
    if (tab && !tab.view) {
      // Reopen the plain editor as a console, keeping unsaved text.
      if (isDirty(tab)) await saveFile(path);
      closeTab(path);
    }
    await openScratch(file);
    return;
  }
  const { requestAttach } = await import("./attach");
  requestAttach(name);
}

export async function renameScratch(name: string, newName: string): Promise<boolean> {
  if (!newName.trim() || newName === name) return false;
  const path = scratchPath(name);
  const tab = editor.tabs.find((t) => t.path === path);
  try {
    if (tab && isDirty(tab)) await saveFile(path);
    const file = await scratchesApi.rename(name, newName.trim());
    await refreshScratches();
    if (tab) {
      closeTab(path);
      await openScratch(file);
    }
    return true;
  } catch (err) {
    showBalloon("Cannot rename scratch", String(err));
    return false;
  }
}

export async function deleteScratch(name: string): Promise<void> {
  const ok = await confirmDialog({
    title: "Delete",
    message: `Delete scratch file "${name}"?`,
    okLabel: "Delete",
    warning: true,
  });
  if (!ok) return;
  try {
    await scratchesApi.delete(name);
    closeTab(scratchPath(name));
    await refreshScratches();
  } catch (err) {
    showBalloon("Cannot delete scratch", String(err));
  }
}

export async function setScratchDataSource(name: string, dataSourceId: string | null): Promise<void> {
  try {
    await scratchesApi.setDataSource(name, dataSourceId);
    await refreshScratches();
  } catch (err) {
    showBalloon("Cannot attach data source", String(err));
  }
}
