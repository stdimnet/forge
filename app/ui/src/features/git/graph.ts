/**
 * Commit graph layout for the Git log: assigns each commit a lane and computes the line
 * segments to draw in its row (PHPStorm-style colored lanes).
 */
import type { Commit } from "./api";

/** A segment inside one row; y is 0 (top), 0.5 (commit dot) or 1 (bottom). */
export interface GraphLine {
  x1: number;
  y1: number;
  x2: number;
  y2: number;
  color: number;
}

export interface GraphRow {
  lane: number;
  color: number;
  /** Number of lanes in use in this row (for the column width). */
  width: number;
  lines: GraphLine[];
}

export function layoutGraph(commits: Pick<Commit, "sha" | "parents">[]): GraphRow[] {
  const lanes: (string | null)[] = [];
  const colors: number[] = [];
  let nextColor = 0;
  const known = new Set(commits.map((c) => c.sha));
  const rows: GraphRow[] = [];

  const freeSlot = () => {
    const i = lanes.indexOf(null);
    return i >= 0 ? i : lanes.length;
  };

  for (const c of commits) {
    const before = lanes.slice();
    const beforeColors = colors.slice();
    let lane = lanes.indexOf(c.sha);
    if (lane < 0) {
      lane = freeSlot();
      lanes[lane] = c.sha;
      colors[lane] = nextColor++;
    }
    const color = colors[lane];
    const lines: GraphLine[] = [];

    // Top half: pass-through lanes and lanes converging into this commit.
    before.forEach((sha, k) => {
      if (sha == null) return;
      if (sha === c.sha) lines.push({ x1: k, y1: 0, x2: lane, y2: 0.5, color: beforeColors[k] });
      else lines.push({ x1: k, y1: 0, x2: k, y2: 0.5, color: beforeColors[k] });
    });
    // Lanes that converged here are free now (except the commit's own).
    before.forEach((sha, k) => {
      if (sha === c.sha && k !== lane) lanes[k] = null;
    });

    // Bottom half: continue to parents.
    const parents = c.parents.filter((p) => known.has(p));
    lanes[lane] = parents[0] ?? null;
    parents.forEach((p, idx) => {
      let target: number;
      if (idx === 0) target = lane;
      else {
        target = lanes.indexOf(p);
        if (target < 0) {
          target = freeSlot();
          lanes[target] = p;
          colors[target] = nextColor++;
        }
      }
      lines.push({ x1: lane, y1: 0.5, x2: target, y2: 1, color: idx === 0 ? color : colors[target] });
    });
    lanes.forEach((sha, k) => {
      if (sha != null && k !== lane && before[k] === sha) lines.push({ x1: k, y1: 0.5, x2: k, y2: 1, color: colors[k] });
    });
    while (lanes.length && lanes[lanes.length - 1] == null) {
      lanes.pop();
      colors.length = lanes.length;
    }
    const width = Math.max(lane + 1, ...lines.map((l) => Math.max(l.x1, l.x2) + 1));
    rows.push({ lane, color, width, lines });
  }
  return rows;
}
