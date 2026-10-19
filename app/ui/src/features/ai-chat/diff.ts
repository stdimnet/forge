/** Line diff (LCS) for the edit cards in the chat. Inputs are small (one edit hunk). */

export interface DiffLine {
  type: "ctx" | "add" | "del";
  text: string;
  oldNo?: number;
  newNo?: number;
}

export function diffLines(oldText: string, newText: string): DiffLine[] {
  const a = oldText ? oldText.split("\n") : [];
  const b = newText ? newText.split("\n") : [];
  // Guard against pathological sizes: fall back to delete-all/add-all.
  if (a.length * b.length > 400_000) {
    return [
      ...a.map((text, i) => ({ type: "del" as const, text, oldNo: i + 1 })),
      ...b.map((text, i) => ({ type: "add" as const, text, newNo: i + 1 })),
    ];
  }
  const n = a.length;
  const m = b.length;
  const lcs: number[][] = Array.from({ length: n + 1 }, () => new Array<number>(m + 1).fill(0));
  for (let i = n - 1; i >= 0; i--)
    for (let j = m - 1; j >= 0; j--) lcs[i][j] = a[i] === b[j] ? lcs[i + 1][j + 1] + 1 : Math.max(lcs[i + 1][j], lcs[i][j + 1]);

  const out: DiffLine[] = [];
  let i = 0;
  let j = 0;
  while (i < n || j < m) {
    if (i < n && j < m && a[i] === b[j]) {
      out.push({ type: "ctx", text: a[i], oldNo: i + 1, newNo: j + 1 });
      i++;
      j++;
    } else if (j < m && (i >= n || lcs[i][j + 1] >= lcs[i + 1][j])) {
      out.push({ type: "add", text: b[j], newNo: j + 1 });
      j++;
    } else {
      out.push({ type: "del", text: a[i], oldNo: i + 1 });
      i++;
    }
  }
  return out;
}

export const diffStats = (lines: DiffLine[]) => ({
  added: lines.filter((l) => l.type === "add").length,
  removed: lines.filter((l) => l.type === "del").length,
});

/** The old/new text pairs an edit tool call describes. */
export function editHunks(name: string, input: Record<string, unknown>): { oldText: string; newText: string }[] {
  const s = (v: unknown) => (typeof v === "string" ? v : "");
  if (name === "Write" || name === "write_file") return [{ oldText: "", newText: s(input.content) }];
  if (name === "MultiEdit" && Array.isArray(input.edits)) {
    return input.edits.map((e: Record<string, unknown>) => ({ oldText: s(e.old_string), newText: s(e.new_string) }));
  }
  return [{ oldText: s(input.old_string), newText: s(input.new_string) }];
}
