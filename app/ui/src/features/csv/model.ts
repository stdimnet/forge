/** Table editor model for CSV/TSV: column titles and the text edit that applies a change. */

/** Column titles: the header row (empty or repeated names made unique) or `C1`, `C2`, … */
export function columnTitles(header: string[] | undefined, columns: number): string[] {
  const seen = new Map<string, number>();
  return Array.from({ length: columns }, (_, i) => {
    const base = header?.[i]?.trim() || `C${i + 1}`;
    const n = seen.get(base) ?? 0;
    seen.set(base, n + 1);
    return n ? `${base} (${n + 1})` : base;
  });
}

/** The smallest single replacement that turns `a` into `b` (one undo step in the editor). */
export function minimalChange(a: string, b: string): { from: number; to: number; insert: string } | null {
  if (a === b) return null;
  let start = 0;
  const max = Math.min(a.length, b.length);
  while (start < max && a[start] === b[start]) start++;
  let endA = a.length;
  let endB = b.length;
  while (endA > start && endB > start && a[endA - 1] === b[endB - 1]) {
    endA--;
    endB--;
  }
  return { from: start, to: endA, insert: b.slice(start, endB) };
}

/** Name of a delimiter for the status line. */
export function delimiterName(d: string): string {
  return ({ ",": "comma", ";": "semicolon", "\t": "tab", "|": "pipe" } as Record<string, string>)[d] ?? `"${d}"`;
}
