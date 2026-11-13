/**
 * Small line diff (LCS) used where git itself is not available: the browser mock and
 * unit tests. Real repositories get their diffs from `git diff`.
 */

export type LineOp = { kind: "same" | "add" | "del"; text: string; a: number; b: number };

const splitLines = (s: string): string[] => {
  if (s === "") return [];
  const lines = s.split("\n");
  if (lines[lines.length - 1] === "") lines.pop();
  return lines;
};

/** Line-level edit script from `a` to `b`. `a`/`b` in each op are 0-based line indexes. */
export function lineDiff(aText: string, bText: string): LineOp[] {
  const a = splitLines(aText);
  const b = splitLines(bText);
  // Trim common prefix/suffix to keep the table small.
  let start = 0;
  while (start < a.length && start < b.length && a[start] === b[start]) start++;
  let endA = a.length;
  let endB = b.length;
  while (endA > start && endB > start && a[endA - 1] === b[endB - 1]) {
    endA--;
    endB--;
  }
  const n = endA - start;
  const m = endB - start;
  const lcs: number[][] = Array.from({ length: n + 1 }, () => new Array<number>(m + 1).fill(0));
  for (let i = n - 1; i >= 0; i--)
    for (let j = m - 1; j >= 0; j--)
      lcs[i][j] = a[start + i] === b[start + j] ? lcs[i + 1][j + 1] + 1 : Math.max(lcs[i + 1][j], lcs[i][j + 1]);

  const ops: LineOp[] = [];
  for (let k = 0; k < start; k++) ops.push({ kind: "same", text: a[k], a: k, b: k });
  let i = 0;
  let j = 0;
  while (i < n || j < m) {
    if (i < n && j < m && a[start + i] === b[start + j]) {
      ops.push({ kind: "same", text: a[start + i], a: start + i, b: start + j });
      i++;
      j++;
    } else if (i < n && (j >= m || lcs[i + 1][j] >= lcs[i][j + 1])) {
      // Deletions before insertions, like git.
      ops.push({ kind: "del", text: a[start + i], a: start + i, b: start + j });
      i++;
    } else {
      ops.push({ kind: "add", text: b[start + j], a: start + i, b: start + j });
      j++;
    }
  }
  for (let k = 0; k < a.length - endA; k++)
    ops.push({ kind: "same", text: a[endA + k], a: endA + k, b: endB + k });
  return ops;
}

/** `git diff`-style unified diff for one file (`null` = file absent on that side). */
export function unifiedDiff(path: string, a: string | null, b: string | null, context = 3): string {
  if (a === b) return "";
  const ops = lineDiff(a ?? "", b ?? "");
  const header = [`diff --git a/${path} b/${path}`];
  if (a === null) header.push("new file mode 100644");
  if (b === null) header.push("deleted file mode 100644");
  header.push(`--- ${a === null ? "/dev/null" : `a/${path}`}`, `+++ ${b === null ? "/dev/null" : `b/${path}`}`);

  const changed = ops.map((o, i) => (o.kind !== "same" ? i : -1)).filter((i) => i >= 0);
  if (!changed.length) return "";
  const hunks: string[] = [];
  let idx = 0;
  while (idx < changed.length) {
    let from = Math.max(0, changed[idx] - context);
    let to = Math.min(ops.length - 1, changed[idx] + context);
    while (idx + 1 < changed.length && changed[idx + 1] - context <= to + 1) {
      idx++;
      to = Math.min(ops.length - 1, changed[idx] + context);
    }
    idx++;
    const slice = ops.slice(from, to + 1);
    const oldLines = slice.filter((o) => o.kind !== "add").length;
    const newLines = slice.filter((o) => o.kind !== "del").length;
    const first = slice[0];
    const oldStart = oldLines ? first.a + 1 : first.a;
    const newStart = newLines ? first.b + 1 : first.b;
    hunks.push(`@@ -${oldStart},${oldLines} +${newStart},${newLines} @@`);
    for (const o of slice) hunks.push(`${o.kind === "add" ? "+" : o.kind === "del" ? "-" : " "}${o.text}`);
    from = to;
  }
  return [...header, ...hunks].join("\n") + "\n";
}
