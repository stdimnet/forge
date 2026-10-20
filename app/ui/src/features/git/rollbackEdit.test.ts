import { describe, expect, it } from "vitest";
import { EditorState, Text } from "@codemirror/state";
import { Chunk } from "@codemirror/merge";
import { lineDiffConfig, rollbackEdit } from "./lineDiff";

const text = (s: string) => Text.of(s.split("\n"));

/** Roll back every chunk, last first, and expect HEAD. */
function rollbackAll(head: string, doc: string): string {
  const h = text(head);
  let state = EditorState.create({ doc });
  const chunks = Chunk.build(h, state.doc, lineDiffConfig);
  for (const c of [...chunks].reverse()) state = state.update({ changes: rollbackEdit(c, h, state.doc) }).state;
  return state.doc.toString();
}

describe("rollbackEdit", () => {
  const cases: [string, string, string][] = [
    ["restores a removed final newline", "a\nb\n", "a\nb"],
    ["removes an added final newline", "a\nb", "a\nb\n"],
    ["restores a deleted line", "a\nb\nc\n", "a\nc\n"],
    ["removes an added line", "a\nc\n", "a\nb\nc\n"],
    ["restores a changed line", "a\nb\n", "a\nx\n"],
    ["restores the last line when it was deleted with its newline", "a\nb\n", "a\n"],
    ["restores a file emptied completely", "x\n", ""],
    ["removes everything added to an empty file", "", "x\ny\n"],
    ["handles changes at both ends", "a\nb\nc\nd\n", "x\nb\nc\ny"],
    ["restores several blank lines at the end", "a\n\n\n", "a"],
    ["drops extra blank lines at the end", "a\n", "a\n\n\n"],
  ];
  for (const [name, head, doc] of cases) {
    it(name, () => expect(rollbackAll(head, doc)).toBe(head));
  }
});
