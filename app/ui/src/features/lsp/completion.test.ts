import { EditorView } from "@codemirror/view";
import { describe, expect, it } from "vitest";
import type { EditorCompletionItem } from "@/ipc/lsp";
import { toCodeMirrorSnippet, toOption } from "./completion";

describe("toCodeMirrorSnippet", () => {
  it("wraps numbered tab stops and turns the final stop into an empty field", () => {
    expect(toCodeMirrorSnippet('"enabled": ${1:true}')).toBe('"enabled": ${1:true}');
    expect(toCodeMirrorSnippet("f($1, $2)$0")).toBe("f(${1}, ${2})${}");
    expect(toCodeMirrorSnippet("a ${0}")).toBe("a ${}");
  });
});

describe("toOption", () => {
  const key: EditorCompletionItem = {
    label: "enabled",
    kind: "property",
    insertText: '"enabled": ${1:true}',
    filterText: '"enabled"',
    replaceRange: { start: { line: 4, character: 2 }, end: { line: 4, character: 6 } },
    raw: { insertTextFormat: 2 },
  };

  it("matches on the filter text but shows the label", () => {
    const option = toOption("renovate.json", key, 0);
    expect(option.label).toBe('"enabled"');
    expect(option.displayLabel).toBe("enabled");
  });

  it("keeps the label when there is no separate filter text", () => {
    const option = toOption("a.php", { label: "count", kind: "function", insertText: "count" }, 0);
    expect(option.label).toBe("count");
    expect(option.displayLabel).toBe("count()");
  });

  it("puts the caret between the parentheses a server inserts with a method name", () => {
    const view = new EditorView({ doc: "$this->calc", selection: { anchor: 11 } });
    const item: EditorCompletionItem = {
      label: "calculateTotal",
      kind: "method",
      insertText: "calculateTotal()",
      replaceRange: { start: { line: 0, character: 11 }, end: { line: 0, character: 11 } },
    };
    const option = toOption("a.php", item, 0);
    expect(option.displayLabel).toBe("calculateTotal()");
    (option.apply as (v: EditorView, c: unknown, from: number, to: number) => void)(view, option, 7, 11);
    expect(view.state.doc.toString()).toBe("$this->calculateTotal()");
    expect(view.state.selection.main.head).toBe(22);
    view.destroy();
  });

  it("replaces the quoted key including the auto-closed quote and starts the snippet", () => {
    const doc = '{\n  "ex"\n}';
    const view = new EditorView({ doc, selection: { anchor: 7 } }); // between `ex` and the closing quote
    const option = toOption("renovate.json", { ...key, replaceRange: { start: { line: 1, character: 2 }, end: { line: 1, character: 6 } } }, 0);
    (option.apply as (v: EditorView, c: unknown, from: number, to: number) => void)(view, option, 4, 7);
    expect(view.state.doc.toString()).toBe('{\n  "enabled": true\n}');
    view.destroy();
  });
});
