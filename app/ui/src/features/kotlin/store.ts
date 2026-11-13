/**
 * KotlinForge state in the UI: the Kotlin setup (language server, ktlint, detekt, kotlinc)
 * and running downloads. Everything is computed in Rust (`commands/kotlin.rs`); this module
 * caches the answers for the settings page and the status bar.
 */
import { createRoot, createSignal } from "solid-js";
import { kotlinApi, type KotlinInstallProgress, type KotlinStatus } from "@/ipc/kotlin";
import { isPluginActive } from "@/state/project";

export const PLUGIN_ID = "forge.kotlin";
export const active = () => isPluginActive(PLUGIN_ID);

const state = createRoot(() => {
  const [status, setStatus] = createSignal<KotlinStatus | null>(null);
  const [install, setInstall] = createSignal<{ id: KotlinInstallProgress["id"]; label: string; done: number; total: number | null } | null>(null);
  return { status, setStatus, install, setInstall };
});

export const kotlinStatus = state.status;
export const activeInstall = state.install;

export async function refreshStatus(): Promise<KotlinStatus | null> {
  const s = await kotlinApi.status().catch(() => null);
  state.setStatus(s);
  return s;
}

let listening = false;

/** Mirror `kotlin://install` progress into the status bar. */
export function listenForInstalls(): void {
  if (listening) return;
  listening = true;
  void kotlinApi.onInstall(({ id, event }) => {
    if (event.kind === "progress") state.setInstall({ id, label: event.label, done: event.done, total: event.total ?? null });
    else if (event.kind === "step") state.setInstall({ id, label: event.message, done: 0, total: null });
  });
}

export const clearInstall = () => state.setInstall(null);

/** `app/src/main/kotlin/com/acme/Main.kt` → `app` (the build module of a source file). */
export function moduleOf(path: string): string {
  const p = path.replace(/\\/g, "/");
  const i = p.lastIndexOf("src/");
  return i > 0 ? p.slice(0, i).replace(/\/$/, "") : "";
}

/** Megabytes for the consent balloon. */
export const megabytes = (bytes: number | null | undefined) => (bytes ? `${Math.round(bytes / 1_000_000)} MB` : "");
