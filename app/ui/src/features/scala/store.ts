/**
 * Scala state in the UI: Metals and sbt as found, the sbt build of the tool window and a
 * running import. Everything is computed in Rust (`commands/scala.rs`); this module caches
 * the answers.
 */
import { createRoot, createSignal } from "solid-js";
import { scalaApi, type SbtBuild, type ScalaStatus, type ToolLine } from "@/ipc/scala";
import { isPluginActive } from "@/state/project";

export const PLUGIN_ID = "forge.scala";
export const active = () => isPluginActive(PLUGIN_ID);

const state = createRoot(() => {
  const [status, setStatus] = createSignal<ScalaStatus | null>(null);
  const [build, setBuild] = createSignal<SbtBuild | null>(null);
  const [importing, setImporting] = createSignal(false);
  const [importError, setImportError] = createSignal<string | null>(null);
  const [log, setLog] = createSignal<ToolLine[]>([]);
  const [installing, setInstalling] = createSignal<string | null>(null);
  return { status, setStatus, build, setBuild, importing, setImporting, importError, setImportError, log, setLog, installing, setInstalling };
});

export const scalaStatus = state.status;
export const sbtBuild = state.build;
export const importing = state.importing;
export const importError = state.importError;
export const importLog = state.log;
export const metalsInstalling = state.installing;
export const setMetalsInstalling = state.setInstalling;

export async function refreshStatus(): Promise<ScalaStatus | null> {
  const s = await scalaApi.status().catch(() => null);
  state.setStatus(s);
  return s;
}

export async function refreshBuild(): Promise<SbtBuild | null> {
  const b = await scalaApi.sbtModel().catch(() => null);
  state.setBuild(b);
  return b;
}

let listening = false;

/** Reload the sbt project (sbt prints projects, Scala versions and dependencies). */
export async function importBuild(): Promise<void> {
  if (state.importing()) return;
  if (!listening) {
    listening = true;
    void scalaApi.onSbt((l) => state.setLog((prev) => [...prev.slice(-199), l]));
  }
  state.setImporting(true);
  state.setImportError(null);
  state.setLog([]);
  try {
    state.setBuild(await scalaApi.sbtImport());
  } catch (e) {
    state.setImportError(String(e).replace(/^Error: /, ""));
  } finally {
    state.setImporting(false);
  }
}

export const clearImportError = () => state.setImportError(null);

/** `modules/api/src/main/scala/A.scala` → the sbt project whose folder holds the file. */
export function projectOf(path: string): string {
  const b = state.build();
  if (!b) return "";
  const p = path.replace(/\\/g, "/");
  const best = b.projects
    .filter((x) => !x.dir || p.startsWith(`${x.dir}/`))
    .sort((a, c) => c.dir.length - a.dir.length)[0];
  return best && best.dir ? best.id : "";
}
