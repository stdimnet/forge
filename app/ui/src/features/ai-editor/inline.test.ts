import { describe, expect, it } from "vitest";
import { EditorState } from "@codemirror/state";
import { nextLine, nextWord, setSuggestion, suggestionField, typedThrough } from "./inline";

describe("inline completion", () => {
  it("splits accepted words and lines", () => {
    expect(nextWord("$this->items->sum()")).toBe("$this");
    expect(nextWord("->items")).toBe("->");
    expect(nextWord("  return 1;")).toBe("  return");
    expect(nextLine("a + b;\nnext")).toBe("a + b;");
    expect(nextLine("\n  run();\n}")).toBe("\n  run();");
  });

  it("keeps the rest of a suggestion the user types through", () => {
    const s = { pos: 10, text: "items.length", alternatives: ["items.length"], index: 0 };
    expect(typedThrough(s, 10, "ite")).toMatchObject({ pos: 13, text: "ms.length" });
    expect(typedThrough(s, 10, "x")).toBeNull();
    expect(typedThrough(s, 11, "t")).toBeNull();
    expect(typedThrough(s, 10, "items.length")).toBeNull();
  });

  it("drops the suggestion when the caret moves away or the text differs", () => {
    let st = EditorState.create({ doc: "let x = ", extensions: [suggestionField], selection: { anchor: 8 } });
    st = st.update({ effects: setSuggestion.of({ pos: 8, text: "items.length;", alternatives: ["items.length;"], index: 0 }) }).state;
    st = st.update({ changes: { from: 8, insert: "it" }, selection: { anchor: 10 }, userEvent: "input.type" }).state;
    expect(st.field(suggestionField)).toMatchObject({ pos: 10, text: "ems.length;" });
    st = st.update({ selection: { anchor: 0 } }).state;
    expect(st.field(suggestionField)).toBeNull();
  });
});
