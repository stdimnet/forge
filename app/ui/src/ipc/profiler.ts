/**
 * Profiler snapshots (`forge-profiler`): Xdebug cachegrind files, SPX reports, V8 CPU
 * profiles, sampling heap profiles and heap snapshots, as execution statistics, callers /
 * callees and the shared call tree / flame graph (`ProfileView`), or the Memory view of a
 * heap snapshot.
 */
import { invoke } from "./index";
import type { ProfileView, ValueType } from "./profile";

export type SnapshotKind = "xdebug" | "spx" | "v8-cpu" | "v8-heap-sampling" | "v8-heap";

/** How a run is profiled (`RunRequest.profiler`). */
export type RunProfiler = "xdebug" | "spx" | "v8-cpu" | "v8-heap";

export interface FunctionStats {
  name: string;
  file: string;
  line: number;
  /** Invocations, or samples for V8 profiles. */
  calls: number;
  own: number;
  total: number;
}

export interface CallEdge {
  caller: number;
  callee: number;
  calls: number;
  total: number;
}

export interface HeapObject {
  id: number;
  name: string;
  className: string;
  shallow: number;
  retained: number;
}

export interface HeapClass {
  name: string;
  count: number;
  shallow: number;
  retained: number;
  objects: HeapObject[];
}

export interface HeapSummary {
  nodeCount: number;
  edgeCount: number;
  totalSize: number;
  objectCount: number;
  unreachableSize: number;
  classes: HeapClass[];
  biggest: HeapObject[];
}

export interface Snapshot {
  kind: SnapshotKind;
  title: string;
  info: { label: string; value: string }[];
  metrics: ValueType[];
  metric: number;
  /** "Calls", "Samples" or "Count". */
  callsLabel: string;
  total: number;
  functions: FunctionStats[];
  edges: CallEdge[];
  view?: ProfileView | null;
  heap?: HeapSummary | null;
}

export interface RecentSnapshot {
  path: string;
  name: string;
  at: number;
}

export const profilerApi = {
  /** Open a snapshot file (absolute local path or project-relative) for a metric. */
  open: (path: string, metric?: number) => invoke<Snapshot>("profiler_open", { path, metric: metric ?? null }),
  /** Snapshots of earlier profiling runs, newest first. */
  recent: () => invoke<RecentSnapshot[]>("profiler_recent"),
};
