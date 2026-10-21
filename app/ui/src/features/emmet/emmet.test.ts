import { describe, expect, it } from "vitest";
import { EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { nextSnippetField } from "@codemirror/autocomplete";
import { indentUnit } from "@codemirror/language";
import { fileTypesReady } from "@/mock/filetypesMock";
import "@/mock/emmetMock";
import { forgeWasm } from "@/wasm";
import { emmetApi } from "@/ipc/emmet";
import { expandAbbreviation, emmetExtension } from "./editor";
import { applyEdits, emmetFamily, emmetOptions, markupEnabled } from "./model";

await fileTypesReady;
await forgeWasm();
await Promise.resolve();

// jsdom has no layout; CodeMirror measures text ranges after scrolling a change into view.
Range.prototype.getClientRects = () => [] as unknown as DOMRectList;
Range.prototype.getBoundingClientRect = () => new DOMRect();

function viewOf(doc: string, path: string): EditorView {
  const pos = doc.indexOf("|");
  return new EditorView({
    state: EditorState.create({
      doc: doc.replace("|", ""),
      selection: { anchor: pos },
      extensions: [indentUnit.of("  "), emmetExtension(path) ?? []],
    }),
  });
}

const sel = (view: EditorView) => view.state.sliceDoc(view.state.selection.main.from, view.state.selection.main.to);

describe("Emmet in the editor", () => {
  it("expands markup abbreviations as a snippet with tab stops", () => {
    const view = viewOf("<body>\n  ul.nav>li*2>a|\n</body>", "index.html");
    expect(expandAbbreviation("index.html", view)).toBe(true);
    expect(view.state.doc.toString()).toBe(
      '<body>\n  <ul class="nav">\n    <li><a href=""></a></li>\n    <li><a href=""></a></li>\n  </ul>\n</body>',
    );
    // The caret is in the first empty href; Tab moves to the link text.
    const head = view.state.selection.main.head;
    expect(view.state.sliceDoc(head - 6, head + 1)).toBe('href=""');
    nextSnippetField(view);
    expect(view.state.sliceDoc(view.state.selection.main.head, view.state.selection.main.head + 4)).toBe("</a>");
    view.destroy();
  });

  it("expands CSS property abbreviations with the value selected", () => {
    const view = viewOf(".a {\n  pos|\n}", "app.scss");
    expect(expandAbbreviation("app.scss", view)).toBe(true);
    expect(view.state.doc.toString()).toBe(".a {\n  position: relative;\n}");
    expect(sel(view)).toBe("relative");
    const v2 = viewOf(".a {\n  m10-20|\n}", "app.css");
    expect(expandAbbreviation("app.css", v2)).toBe(true);
    expect(v2.state.doc.toString()).toBe(".a {\n  margin: 10px 20px;\n}");
    view.destroy();
    v2.destroy();
  });

  it("uses className in JSX and leaves plain code alone", () => {
    const view = viewOf("return (\n  .card>h2|\n);", "Card.tsx");
    expect(expandAbbreviation("Card.tsx", view)).toBe(true);
    expect(view.state.doc.toString()).toBe('return (\n  <div className="card">\n    <h2></h2>\n  </div>\n);');
    const code = viewOf("const total|", "a.ts");
    expect(expandAbbreviation("a.ts", code)).toBe(false);
    const php = viewOf("<?php\n$x = div|", "a.php");
    expect(expandAbbreviation("a.php", php)).toBe(false);
    for (const v of [view, code, php]) v.destroy();
  });

  it("knows the file families and default options", () => {
    expect(emmetFamily("blade")).toBe("html");
    expect(emmetFamily("tsx")).toBe("jsx");
    expect(emmetFamily("less")).toBe("css");
    expect(emmetFamily("python")).toBeNull();
    expect(markupEnabled("twig")).toBe(true);
    expect(markupEnabled("css")).toBe(false);
    expect(emmetOptions()).toMatchObject({ html: true, css: true, jsx: true, jsxClassAttr: "className" });
    expect(emmetExtension("README.md")).toBeNull();
  });

  it("wraps, updates and removes tags through the mock", async () => {
    const text = "<ul>\n  <li>Home</li>\n</ul>";
    const options = emmetOptions();
    const wrapped = await emmetApi.wrap({ text, from: 7, to: 20, abbreviation: "div.item", languageId: "html", options });
    expect(wrapped.text).toBe('<div class="item"><li>Home</li></div>');
    await expect(emmetApi.wrap({ text, from: 7, to: 20, abbreviation: "div>", languageId: "html", options })).rejects.toThrow();
    const view = viewOf("<ul>\n  <li>Ho|me</li>\n</ul>", "index.html");
    applyEdits(view, await emmetApi.updateTag({ text: view.state.doc.toString(), offset: 12, abbreviation: "li.active", options }));
    expect(view.state.doc.toString()).toBe('<ul>\n  <li class="active">Home</li>\n</ul>');
    const removed = await emmetApi.removeTag({ text: "<p>a <b>b</b></p>", offset: 9, options });
    expect(removed.map((e) => [e.from, e.to])).toEqual([
      [5, 8],
      [9, 13],
    ]);
    expect(await emmetApi.editPoint({ text: '<a href=""></a>', offset: 0, forward: true })).toBe(9);
    view.destroy();
  });
});
