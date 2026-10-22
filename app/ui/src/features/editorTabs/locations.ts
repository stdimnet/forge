/**
 * Recent Locations (Ctrl+Shift+E): the places the caret visited or text was changed, newest
 * first, like PHPStorm's IDE document history. A place is a file and a 1-based line; places
 * within a few lines of each other in one file count as one.
 */

export interface Location {
  path: string;
  /** 1-based line. */
  line: number;
  /** Text was changed here (shown with "Show changed only"). */
  changed: boolean;
  /** Recording order, larger is newer. */
  seq: number;
}

/** Places closer than this (in lines) merge. */
export const MERGE_DISTANCE = 3;
export const MAX_LOCATIONS = 50;

let seq = 0;

/** Record a visit or change; returns the new list (newest first). */
export function recordLocation(list: Location[], path: string, line: number, changed: boolean): Location[] {
  const near = list.find((l) => l.path === path && Math.abs(l.line - line) <= MERGE_DISTANCE);
  const entry: Location = { path, line, changed: changed || !!near?.changed, seq: ++seq };
  return [entry, ...list.filter((l) => l !== near)].slice(0, MAX_LOCATIONS);
}

/** Shift the lines of a file's places after lines were inserted (`delta` > 0) or removed below `line`. */
export function shiftLocations(list: Location[], path: string, line: number, delta: number): Location[] {
  if (!delta) return list;
  return list.map((l) => (l.path === path && l.line > line ? { ...l, line: Math.max(line, l.line + delta) } : l));
}

/** Drop places of files that were deleted; follow renames. */
export function movePath(list: Location[], from: string, to: string | null): Location[] {
  const under = (p: string) => p === from || p.startsWith(`${from}/`);
  return to === null
    ? list.filter((l) => !under(l.path))
    : list.map((l) => (under(l.path) ? { ...l, path: to + l.path.slice(from.length) } : l));
}

/** The lines around a place for the popup: `[lineNumber, text]`, the place in the middle. */
export function snippet(text: string, line: number, before = 1, after = 1): [number, string][] {
  const lines = text.split("\n");
  const at = Math.min(Math.max(1, line), lines.length);
  const out: [number, string][] = [];
  for (let n = Math.max(1, at - before); n <= Math.min(lines.length, at + after); n++) out.push([n, lines[n - 1]]);
  return out;
}
