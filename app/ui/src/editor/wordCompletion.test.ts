import { describe, expect, it } from "vitest";
import { EditorState } from "@codemirror/state";
import type { CompletionContext } from "@codemirror/autocomplete";
import { explicitWords, wordCompletion } from "./wordCompletion";

/** A completion context at the `|` marker of `doc`. */
function contextAt(doc: string, explicit: boolean): CompletionContext {
  const pos = doc.indexOf("|");
  const state = EditorState.create({ doc: doc.replace("|", "") });
  return {
    state,
    pos,
    explicit,
    matchBefore(re: RegExp) {
      const line = state.doc.lineAt(pos);
      const m = new RegExp(`(?:${re.source})$`, re.flags).exec(state.sliceDoc(line.from, pos));
      return m ? { from: line.from + m.index, to: pos, text: m[0] } : null;
    },
  } as unknown as CompletionContext;
}

describe("word completion", () => {
  it("offers words of the document when invoked explicitly", async () => {
    const res = await explicitWords(contextAt("fooBar fooBaz foo|", true));
    expect(res?.options.map((o) => o.label).sort()).toEqual(["fooBar", "fooBaz"]);
  });
  it("stays quiet while typing", () => {
    expect(explicitWords(contextAt("fooBar foo|", false))).toBeNull();
  });
  it("is registered for every file", () => {
    const state = EditorState.create({ doc: "x", extensions: wordCompletion });
    expect(state.languageDataAt("autocomplete", 0)).toContain(explicitWords);
  });
});
