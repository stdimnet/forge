/**
 * The Profiler tool window's state: the project's cProfile snapshots (newest first) and the
 * shown one's call tree and hot spots, read in Rust (`pyforge_testing::pstats`). A finished
 * Profile run shows its snapshot.
 */
import { createSignal } from "solid-js";
import { pythonToolsApi, type ProfileInfo, type ProfileSnapshot } from "@/ipc/pythonTools";
import { onProfileSnapshot } from "@/features/run";
import { showToolWindow } from "@/state/layout";
import { showBalloon } from "@/keymap";

const [snapshots, setSnapshots] = createSignal<ProfileInfo[]>([]);
const [current, setCurrent] = createSignal<ProfileSnapshot | null>(null);
const [loading, setLoading] = createSignal(false);
export { snapshots, current as currentSnapshot, loading as profileLoading };

export async function refreshSnapshots(): Promise<void> {
  setSnapshots(await pythonToolsApi.profiles().catch(() => []));
}

export async function showSnapshot(id: string): Promise<void> {
  setLoading(true);
  try {
    setCurrent(await pythonToolsApi.profileView(id));
  } catch (e) {
    showBalloon("Cannot open the profile", String(e).replace(/^Error: /, ""), 6000, undefined, { type: "error" });
  } finally {
    setLoading(false);
  }
}

/** Open the newest snapshot when nothing is shown yet. */
export async function loadLatest(): Promise<void> {
  await refreshSnapshots();
  const first = snapshots()[0];
  if (first && !current()) await showSnapshot(first.id);
}

export async function deleteSnapshot(id: string): Promise<void> {
  await pythonToolsApi.profileDelete(id).catch(() => undefined);
  if (current()?.info.id === id) setCurrent(null);
  await loadLatest();
}

export async function openSnapshotFile(path: string): Promise<void> {
  try {
    const info = await pythonToolsApi.profileOpen(path);
    await refreshSnapshots();
    await showSnapshot(info.id);
    showToolWindow("profiler");
  } catch (e) {
    showBalloon("Cannot open the profile", String(e).replace(/^Error: /, ""), 6000, undefined, { type: "error" });
  }
}

onProfileSnapshot((info) => {
  void refreshSnapshots().then(() => showSnapshot(info.id));
  showToolWindow("profiler");
});
