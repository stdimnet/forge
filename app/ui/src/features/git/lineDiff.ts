/**
 * Line-aligned diff for @codemirror/merge (`DiffConfig.override`). The stock diff works on
 * characters, so an inserted method can show up as a "modified" line that starts mid-way
 * through its neighbour. Like PHPStorm we first diff whole lines, then refine each modified
 * block character-wise for the word highlights.
 */
import { Change, diff, presentableDiff, type Chunk } from "@codemirror/merge";
import type { Text } from "@codemirror/state";

/** Split keeping line terminators, so offsets can be summed back. */
function lines(s: string): string[] {
  const out: string[] = [];
  let start = 0;
  for (let i = 0; i < s.length; i++) {
    if (s.charCodeAt(i) === 10) {
      out.push(s.slice(start, i + 1));
      start = i + 1;
    }
  }
  if (start < s.length) out.push(s.slice(start));
  return out;
}

/** Map each distinct line to one character so a string diff becomes a line diff. */
function encode(a: string[], b: string[]): [string, string] {
  const ids = new Map<string, number>();
  const code = (line: string) => {
    let id = ids.get(line);
    if (id === undefined) {
      id = ids.size;
      ids.set(line, id);
    }
    // Skip the surrogate range so every line stays a single UTF-16 unit.
    const c = 0x100 + id;
    return String.fromCharCode(c >= 0xd800 ? c + 0x800 : c);
  };
  return [a.map(code).join(""), b.map(code).join("")];
}

function offsets(ls: string[]): number[] {
  const out = [0];
  for (const l of ls) out.push(out[out.length - 1] + l.length);
  return out;
}

export function lineAlignedDiff(a: string, b: string): readonly Change[] {
  const la = lines(a);
  const lb = lines(b);
  if (la.length + lb.length > 60000) return presentableDiff(a, b);
  const [ea, eb] = encode(la, lb);
  const oa = offsets(la);
  const ob = offsets(lb);
  const result: Change[] = [];
  for (const c of diff(ea, eb, { scanLimit: 5000 })) {
    const fromA = oa[c.fromA];
    const toA = oa[c.toA];
    const fromB = ob[c.fromB];
    const toB = ob[c.toB];
    if (fromA === toA || fromB === toB) {
      result.push(new Change(fromA, toA, fromB, toB));
      continue;
    }
    // Modified block: refine to word level inside it.
    for (const r of presentableDiff(a.slice(fromA, toA), b.slice(fromB, toB)))
      result.push(new Change(fromA + r.fromA, fromA + r.toA, fromB + r.fromB, fromB + r.toB));
  }
  return result;
}

export const lineDiffConfig = { override: lineAlignedDiff, scanLimit: 5000 };

/**
 * The edit that restores one chunk of `doc` to `head`. Chunk ends include the line break
 * (and may point one past a document that has no trailing newline), so both sides are only
 * clamped; this also restores or removes a final newline at the end of the file.
 */
export function rollbackEdit(chunk: Chunk, head: Text, doc: Text): { from: number; to: number; insert: string } {
  return {
    from: chunk.fromB,
    to: Math.min(chunk.toB, doc.length),
    insert: head.sliceString(chunk.fromA, Math.min(chunk.toA, head.length)),
  };
}
