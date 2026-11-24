import { describe, expect, it } from "vitest";
import { EditorState, type Extension } from "@codemirror/state";
import { insertNewlineAndIndent } from "@codemirror/commands";
import { StreamLanguage } from "@codemirror/language";
import { kotlin } from "@codemirror/legacy-modes/mode/clike";
import { php } from "@codemirror/lang-php";
import { javascript } from "@codemirror/lang-javascript";
import type { FileFormat } from "@/ipc/text";
import { formatExtensions } from "./codeStyle";

const format = (useTabs: boolean, size = 4) =>
  ({ indent: { useTabs, size, tabSize: 4, continuation: 8 }, hardWrapAt: 0, visualGuides: [] }) as unknown as FileFormat;

/** Press Enter at `|` and return the document with `|` at the new caret. */
function enter(doc: string, language: Extension, fmt: FileFormat): string {
  let state = EditorState.create({
    doc: doc.replace("|", ""),
    selection: { anchor: doc.indexOf("|") },
    extensions: [language, formatExtensions(fmt, "    ")],
  });
  insertNewlineAndIndent({ state, dispatch: (tr) => (state = tr.state) });
  const head = state.selection.main.head;
  return state.doc.sliceString(0, head) + "|" + state.doc.sliceString(head);
}

const kt = StreamLanguage.define(kotlin);

describe("Enter in a file with a resolved code style", () => {
  it("indents one level after an opening brace, with tabs or spaces", () => {
    expect(enter("class A {|\n}", kt, format(true))).toBe("class A {\n\t|\n}");
    expect(enter("class A {|\n}", kt, format(false))).toBe("class A {\n    |\n}");
    expect(enter("class A {\n\tfun b() {|\n\t}\n}", kt, format(true))).toBe("class A {\n\tfun b() {\n\t\t|\n\t}\n}");
    expect(enter("class A {\n  fun b() {|\n  }\n}", kt, format(false, 2))).toBe("class A {\n  fun b() {\n    |\n  }\n}");
  });

  it("keeps the indentation of the line before otherwise", () => {
    expect(enter("class A {\n\tfun b() {\n\t\tc()|\n\t}\n}", kt, format(true))).toBe(
      "class A {\n\tfun b() {\n\t\tc()\n\t\t|\n\t}\n}",
    );
    expect(enter("class A {\n    val b = 1|\n}", kt, format(false))).toBe("class A {\n    val b = 1\n    |\n}");
  });

  it("indents between braces typed on one line", () => {
    expect(enter("fun a() {|}", kt, format(true))).toBe("fun a() {\n\t|\n}");
  });

  it("indents other languages too", () => {
    expect(enter("<?php\nclass A {\n\tfunction b() {|\n\t}\n}", php(), format(true))).toBe(
      "<?php\nclass A {\n\tfunction b() {\n\t\t|\n\t}\n}",
    );
    expect(enter("function a() {\n  if (b) {|\n  }\n}", javascript({ typescript: true }), format(false, 2))).toBe(
      "function a() {\n  if (b) {\n    |\n  }\n}",
    );
  });

  it("still indents method chains by the continuation indent", () => {
    expect(enter("<?php\n$a = $b|->c();", php(), format(false))).toBe("<?php\n$a = $b\n        |->c();");
  });
});
