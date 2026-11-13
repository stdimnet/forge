/**
 * The shared Profiler's data (`goforge_pprof::ProfileView`): the sample types, the top
 * functions, and the call tree the flame graph is drawn from. GoForge's pprof profiles and
 * CForge's perf profiles both use it.
 */
export interface ValueType {
  kind: string;
  unit: string;
}

export interface TopRow {
  name: string;
  file: string;
  line: number;
  flat: number;
  cum: number;
}

export interface CallNode {
  name: string;
  file: string;
  line: number;
  total: number;
  self: number;
  children: CallNode[];
}

export interface ProfileView {
  sampleTypes: ValueType[];
  sampleIndex: number;
  total: number;
  samples: number;
  durationNanos: number;
  periodType?: ValueType | null;
  period: number;
  top: TopRow[];
  tree: CallNode;
  pruned: number;
}
