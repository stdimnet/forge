/** The open project's trust state (Safe Mode), kept in sync with Rust. */
import { createEffect, createRoot, createSignal } from "solid-js";
import { trustApi, type TrustStatus } from "@/ipc/trust";
import { workspace } from "@/state/workspace";

const [status, setStatus] = createSignal<TrustStatus | null>(null);

/** The open project's trust state (`null` without a project). */
export const trustStatus = status;

/** The open project is in Safe Mode: nothing of it runs. */
export const safeMode = (): boolean => !!status()?.safeMode;

export async function refreshTrust(): Promise<void> {
  setStatus(workspace() ? await trustApi.status().catch(() => null) : null);
}

createRoot(() => {
  createEffect(() => {
    workspace();
    void refreshTrust();
  });
});
void trustApi.onChanged((s) => setStatus(workspace() ? s : null));
