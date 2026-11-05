/** The open project's trust state (Safe Mode), kept in sync with Rust. */
import { createEffect, createRoot, createSignal } from "solid-js";
import { trustApi, type TrustStatus } from "@/ipc/trust";
import { workspace } from "@/state/workspace";
import { onProject } from "@/state/projectLifecycle";

const [status, setStatus] = createSignal<TrustStatus | null>(null);

/** The open project's trust state (`null` without a project). */
export const trustStatus = status;

/** The open project is in Safe Mode: nothing of it runs. */
export const safeMode = (): boolean => !!status()?.safeMode;

/**
 * The Safe Mode banner and Trust Project… apply: a local or remote project that is not
 * trusted. A Code With Me guest's window is in Safe Mode too, but there is nothing to trust
 * there (the host's project runs on the host).
 */
export const canTrustProject = (): boolean => {
  const s = status();
  return !!s && s.safeMode && !s.trusted && !s.guest;
};

let generation = 0;

export async function refreshTrust(): Promise<void> {
  // Only the latest answer counts: the previous project's may arrive after the next one's.
  const mine = ++generation;
  if (!workspace()) return void setStatus(null);
  const next = await trustApi.status().catch(() => null);
  if (mine === generation) setStatus(workspace() ? next : null);
}

// The previous project's state (its Safe Mode banner) goes with it, before the new one is asked.
onProject("closing", () => {
  generation++;
  setStatus(null);
}, { name: "trust" });

createRoot(() => {
  createEffect(() => {
    workspace();
    void refreshTrust();
  });
});
void trustApi.onChanged((s) => setStatus(workspace() ? s : null));
