/**
 * The snapshots shown by the Profiler's Xdebug, SPX, V8 CPU and V8 Heap sources: the last
 * one each source opened (with its file, so another metric can be read), opened from a
 * file or delivered by a profiling run.
 */
import { createSignal } from "solid-js";
import { createStore } from "solid-js/store";
import { profilerApi, type Snapshot, type SnapshotKind } from "@/ipc/profiler";
import { showProfiler } from "@/features/profiler";
import { balloonsFor } from "@/keymap";

export type SourceId = "xdebug" | "spx" | "v8-cpu" | "v8-heap";

export const SOURCE_LABELS: Record<SourceId, string> = {
  xdebug: "Xdebug",
  spx: "SPX",
  "v8-cpu": "V8 CPU",
  "v8-heap": "V8 Heap",
};

export const showBalloon = balloonsFor("Profiler");

export const sourceOf = (kind: SnapshotKind): SourceId => (kind === "v8-heap-sampling" ? "v8-heap" : kind);

interface Loaded {
  snapshot: Snapshot;
  path: string;
}

const [loaded, setLoaded] = createStore<Partial<Record<SourceId, Loaded>>>({});
const [busy, setBusy] = createSignal<SourceId | null>(null);
export { busy as profilerBusy };

export const loadedSnapshot = (source: SourceId): Loaded | undefined => loaded[source];

/** Show a snapshot in its source and bring up the Profiler. */
export function showSnapshot(snapshot: Snapshot, path: string): void {
  const source = sourceOf(snapshot.kind);
  setLoaded(source, { snapshot, path });
  showProfiler(source);
}

/** Read a snapshot file and show it. */
export async function openSnapshot(path: string, metric?: number): Promise<boolean> {
  try {
    const snapshot = await profilerApi.open(path, metric);
    showSnapshot(snapshot, path);
    return true;
  } catch (e) {
    showBalloon("Cannot open the profiler snapshot", String(e).replace(/^Error: /, ""), 8000);
    return false;
  }
}

/** Show another metric of a source's snapshot (Time / Memory, CPU time / Samples). */
export async function switchMetric(source: SourceId, metric: number): Promise<void> {
  const l = loaded[source];
  if (!l || l.snapshot.metric === metric) return;
  setBusy(source);
  try {
    const snapshot = await profilerApi.open(l.path, metric);
    setLoaded(source, { snapshot, path: l.path });
  } catch (e) {
    showBalloon("Cannot read the profiler snapshot", String(e).replace(/^Error: /, ""), 8000);
  } finally {
    setBusy(null);
  }
}
