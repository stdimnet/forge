import type { FileMatches, LineMatch } from "@/ipc/find";

export type MatchRows =
  | { kind: "match"; file: FileMatches; match: LineMatch }
  | { kind: "context"; file: FileMatches; line: number; text: string };

/** The rows of one file: its matching lines with their context lines, each line shown once. */
export function fileRows(file: FileMatches): MatchRows[] {
  const byLine = new Map<number, MatchRows>();
  for (const match of file.matches) byLine.set(match.line, { kind: "match", file, match });
  for (const match of file.matches) {
    const before = match.before ?? [];
    before.forEach((text, i) => {
      const line = match.line - before.length + i;
      if (line >= 0 && !byLine.has(line)) byLine.set(line, { kind: "context", file, line, text });
    });
    (match.after ?? []).forEach((text, i) => {
      const line = match.line + 1 + i;
      if (!byLine.has(line)) byLine.set(line, { kind: "context", file, line, text });
    });
  }
  return [...byLine.entries()].sort((a, b) => a[0] - b[0]).map(([, row]) => row);
}
