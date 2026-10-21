import { describe, expect, it } from "vitest";
import { EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { nextSnippetField } from "@codemirror/autocomplete";
import { indentUnit } from "@codemirror/language";
import { fileTypesReady } from "@/mock/filetypesMock";
import "@/mock/liveTemplatesMock";
import { forgeWasm } from "@/wasm";
import { liveTemplatesApi, type LiveTemplate } from "@/ipc/liveTemplates";
import { entryFor, mergeVariables, removeEntry, replaceEntry, restoreEntry, toggleContext, toSnippet } from "./model";
import { expandAbbreviation, liveTemplatesExtension } from "./editor";
import { filterItems } from "./Popup";

await fileTypesReady;
await forgeWasm();
await Promise.resolve();

// jsdom has no layout; CodeMirror measures text ranges after scrolling a change into view.
Range.prototype.getClientRects = () => [] as unknown as DOMRectList;
Range.prototype.getBoundingClientRect = () => new DOMRect();

const template = (over: Partial<LiveTemplate>): LiveTemplate => ({
  id: "user/x",
  group: "user",
  abbreviation: "x",
  description: "",
  text: "",
  variables: [],
  contexts: ["PHP"],
  applicable: "PHP",
  enabled: true,
  modified: false,
  ...over,
});

describe("toSnippet", () => {
  it("turns tab stops into numbered fields and escapes braces", () => {
    const s = toSnippet({
      from: 0,
      to: 0,
      text: "foreach ($items as $item) {\n    \n}",
      fields: [
        { from: 9, to: 15, index: 1 },
        { from: 20, to: 24, index: 2 },
      ],
      end: 32,
      templateId: "PHP/fore",
    });
    expect(s).toBe("foreach (${1:$items} as $${2:item}) \\{\n    ${0}\n\\}");
  });

  it("keeps values with braces as text after an empty field", () => {
    const s = toSnippet({ from: 0, to: 0, text: "a {b} c", fields: [{ from: 2, to: 5, index: 1 }], end: 7, templateId: "t" });
    expect(s).toBe("a ${1:}\\{b\\} c${0}");
  });
});

describe("expansion in the editor", () => {
  it("expands an abbreviation with Rust and selects the first variable", () => {
    const doc = "<?php\nfunction f() {\n    $orders = [];\n    fore\n}";
    const view = new EditorView({
      state: EditorState.create({ doc, selection: { anchor: doc.indexOf("fore") + 4 }, extensions: [indentUnit.of("    "), liveTemplatesExtension("src/A.php")] }),
    });
    expect(liveTemplatesApi.expandSync({ text: "x", offset: 1, path: "a.php", settings: { custom: [], disabled: [], frameworks: [] }, env: { nowMs: 0, tzOffsetMinutes: 0, clipboard: "", user: "", indentUnit: "    " } })).toBeNull();
    expect(expandAbbreviation("src/A.php", view)).toBe(true);
    expect(view.state.doc.toString()).toBe("<?php\nfunction f() {\n    $orders = [];\n    foreach ($orders as $order) {\n        \n    }\n}");
    const sel = view.state.selection.main;
    expect(view.state.sliceDoc(sel.from, sel.to)).toBe("$orders");
    nextSnippetField(view);
    const next = view.state.selection.main;
    expect(view.state.sliceDoc(next.from, next.to)).toBe("order");
    view.destroy();
  });

  it("leaves the key alone without an abbreviation", () => {
    const view = new EditorView({ state: EditorState.create({ doc: "<?php\n$x = 1;", selection: { anchor: 13 } }) });
    expect(expandAbbreviation("src/A.php", view)).toBe(false);
    view.destroy();
  });

  it("lists templates and unwraps statements through the mock", async () => {
    const applicable = await liveTemplatesApi.applicable({
      text: "<?php\nclass A {\n    pu\n}",
      offset: 22,
      path: "A.php",
      languageId: "php",
      settings: { custom: [], disabled: [], frameworks: [] },
    });
    expect(applicable.prefix).toBe("pu");
    expect(applicable.contexts).toEqual(["PHP_CLASS_MEMBER"]);
    expect(applicable.items.map((i) => i.abbreviation)).toContain("pubf");
    const unwrap = await liveTemplatesApi.unwrap({ text: "<?php\nif ($a) {\n    b();\n}", offset: 20, path: "A.php", languageId: "php" });
    expect(unwrap[0].label).toBe("Unwrap 'if...'");
    expect(unwrap[0].edits[0].text).toBe("b();");
  });
});

describe("settings entries", () => {
  it("replaces, removes and restores templates by identity", () => {
    const fore = template({ id: "PHP/fore", group: "PHP", abbreviation: "fore", builtin: "PHP/fore" });
    let custom = replaceEntry([], fore, entryFor({ ...fore, abbreviation: "fe" }));
    expect(custom).toEqual([{ group: "PHP", abbreviation: "fe", description: "", text: "", contexts: "PHP", builtin: "PHP/fore" }]);
    custom = replaceEntry(custom, { ...fore, id: "PHP/fe" }, entryFor({ ...fore, abbreviation: "fx" }));
    expect(custom).toHaveLength(1);
    expect(removeEntry(custom, { ...fore, id: "PHP/fx" })).toEqual([{ builtin: "PHP/fore", group: "PHP", abbreviation: "fore", removed: "true" }]);
    expect(restoreEntry(custom, { ...fore, modified: true })).toEqual([]);
    const mine = template({ key: "k1" });
    const withMine = replaceEntry([], mine, entryFor(mine));
    const renamed = replaceEntry(withMine, mine, entryFor({ ...mine, abbreviation: "y" }));
    expect(renamed).toEqual([{ group: "user", abbreviation: "y", description: "", text: "", contexts: "PHP", key: "k1" }]);
    expect(removeEntry(renamed, mine)).toEqual([]);
  });

  it("merges variables and toggles contexts like PHPStorm's tree", () => {
    const vars = mergeVariables(["A", "B"], [{ name: "B", expression: "user()", defaultValue: "", alwaysStopAt: false }]);
    expect(vars.map((v) => [v.name, v.expression])).toEqual([
      ["A", ""],
      ["B", "user()"],
    ]);
    const php = { id: "PHP", children: ["PHP_CLASS_MEMBER", "PHP_COMMENT", "PHP_EXPRESSION", "PHP_STATEMENT", "PHP_STRING"] };
    expect(toggleContext([], "PHP", null, true)).toEqual(["PHP"]);
    expect(toggleContext(["PHP"], "PHP_STATEMENT", php, false).sort()).toEqual(["PHP_CLASS_MEMBER", "PHP_EXPRESSION"]);
    expect(toggleContext(["HTML"], "PHP_STATEMENT", php, true)).toEqual(["HTML", "PHP_STATEMENT"]);
  });

  it("filters popup items by prefix first", () => {
    const item = (label: string, detail = "") => ({ id: label, label, detail, run: () => undefined });
    const items = [item("eco", "echo"), item("fore", "foreach"), item("forek", "foreach with key"), item("thr", "throw new")];
    expect(filterItems(items, "for").map((i) => i.label)).toEqual(["fore", "forek"]);
    expect(filterItems(items, "new").map((i) => i.label)).toEqual(["thr"]);
    expect(filterItems(items, "")).toHaveLength(4);
  });
});
