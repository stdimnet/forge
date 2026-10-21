import { afterEach, describe, expect, it } from "vitest";
import { StringStream, foldable, type StreamParser } from "@codemirror/language";
import { EditorSelection, EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { toggleComment } from "@codemirror/commands";
import { CompletionContext, closeBrackets } from "@codemirror/autocomplete";
import builtinPlugins from "@/mock/builtinPlugins.json";
import { setLocalSetting } from "@/state/settings";
import { smartySyntaxWith, templateParser } from "./templates";
import { DATA_LANGUAGES } from "./templateData";
import { DELIMITERS_KEY, syntaxWithDelimiters } from "./templateDelimiters";
import { scanTemplate, templateOverlay, templateRanges, templateSyntaxField } from "./templateOverlay";
import { matchingDelimiters, openBlocksAt, templateEditing, templateTagCompletion } from "./templateEditing";
import { languageFor } from "./languages";
import { fileTypeOf } from "./fileTypes";
import { fileTypesReady } from "@/mock/filetypesMock";

await fileTypesReady;

function tokens<S>(parser: StreamParser<S>, doc: string): [string, string | null][] {
  const state = parser.startState!(2);
  const out: [string, string | null][] = [];
  for (const line of doc.split("\n")) {
    const stream = new StringStream(line, 4, 2);
    while (!stream.eol()) {
      const style = parser.token(stream, state);
      const text = stream.current();
      if (text.trim()) out.push([text, style ?? null]);
      stream.start = stream.pos;
    }
  }
  return out;
}

const dataParser = (id: string) => DATA_LANGUAGES.find((l) => l.id === id)!.parser!;

/** The default value of a setting in the built-in manifests (browser snapshot). */
function manifestDefault(key: string): unknown {
  for (const plugin of builtinPlugins as { contributions?: { settings?: { key: string; default?: unknown }[] } }[]) {
    const spec = plugin.contributions?.settings?.find((s) => s.key === key);
    if (spec) return spec.default;
  }
  return undefined;
}

const smartyLatex = { template: "smarty", language: "latex", left: "[[", right: "]]" };
const twigLatex = { template: "twig", language: "latex", variableStart: "\\VAR{", variableEnd: "}", blockStart: "\\BLOCK{", blockEnd: "}", commentStart: "\\#{", commentEnd: "}" };

describe("delimiter resolution (Rust, through the file type)", () => {
  afterEach(() => setLocalSetting(DELIMITERS_KEY, []));

  it("uses the language defaults without an entry", () => {
    // Without settings (no schema loaded) LaTeX gets no special treatment.
    expect(fileTypeOf("invoice.tex.tpl").delimiters).toEqual({ left: "{", right: "}" });
    expect(fileTypeOf("letter.tex.twig").delimiters).toMatchObject({ variableStart: "{{", blockStart: "{%", commentStart: "{#" });
  });

  it("applies the entry of the (template, data language) pair", () => {
    setLocalSetting(DELIMITERS_KEY, [smartyLatex, twigLatex]);
    expect(fileTypeOf("invoice.tex.tpl").delimiters).toEqual({ left: "[[", right: "]]" });
    expect(fileTypeOf("cart.tpl").delimiters).toEqual({ left: "{", right: "}" });
    expect(fileTypeOf("letter.tex.twig").delimiters).toMatchObject({ variableStart: "\\VAR{", blockStart: "\\BLOCK{", commentEnd: "}" });
    expect(fileTypeOf("page.html.twig").delimiters).toMatchObject({ variableStart: "{{" });
    // Jinja has no entry here, and Latte's delimiters are fixed.
    expect(fileTypeOf("letter.tex.j2").delimiters).toMatchObject({ variableStart: "{{" });
    expect(fileTypeOf("page.latte").delimiters).toBeNull();
  });

  it("ships the LaTeX entries as manifest data, not as code", () => {
    expect(manifestDefault(DELIMITERS_KEY)).toEqual([smartyLatex, twigLatex, { ...twigLatex, template: "jinja" }].map((e) => expect.objectContaining(e)));
  });
});

describe("Smarty with LaTeX and [[ ]]", () => {
  const syntax = smartySyntaxWith("[[", "]]");
  const latex = dataParser("latex");

  it("keeps LaTeX braces LaTeX and styles only [[ ]] tags", () => {
    const t = tokens(templateParser(syntax, latex), "\\textbf{[[$name|upper]]} \\amount{#1}");
    expect(t).toContainEqual(["\\textbf", "tag"]);
    expect(t).toContainEqual(["[[", "meta"]);
    expect(t).toContainEqual(["$name", "variableName"]);
    expect(t).toContainEqual(["|upper", "variableName.function"]);
    expect(t).toContainEqual(["]]", "meta"]);
    // The braces belong to LaTeX, not to Smarty.
    const braces = t.filter(([text]) => text === "{" || text === "}");
    expect(braces.length).toBe(4);
    expect(braces.every(([, style]) => style !== "meta")).toBe(true);
  });

  it("leaves single LaTeX brackets and `[[ ` (auto_literal) alone, like Smarty", () => {
    const doc = "\\\\[2pt] \\item[a] \\includegraphics[width=2cm]{x} [[ not a tag ]] \\item[[$label]]";
    const tags = scanTemplate(doc, syntax).tags.map((t) => doc.slice(t.from, t.to));
    // Only the last one is a tag, as Smarty itself would see it: `\item[[a]]` needs `\item[{[a]}]`.
    expect(tags).toEqual(["[[$label]]"]);
  });

  it("does not let a `%` inside a tag start a LaTeX comment", () => {
    const t = tokens(templateParser(syntax, latex), '\\amount{[[$total|string_format:"%.2f"]]} \\\\ % note');
    expect(t).toContainEqual(['"%.2f"', "string"]);
    expect(t.find(([text]) => text === "}")?.[1]).not.toBe("comment");
    expect(t[t.length - 1]).toEqual(["% note", "comment"]);
  });

  it("follows the delimiters for comments, closing tags and literal blocks", () => {
    const doc = "[[* note {x} *]]\n[[if $a]]x[[/if]]\n[[literal]][[$no]][[/literal]]";
    const ranges = templateRanges(doc, syntax).map((r) => [doc.slice(r.from, r.to), r.style]);
    expect(ranges).toContainEqual([" note {x} *]]", "comment"]);
    expect(ranges).toContainEqual(["if", "keyword"]);
    expect(ranges).toContainEqual(["[[/", "meta"]);
    expect(ranges).not.toContainEqual(["$no", "variableName"]);
    const { tags } = scanTemplate(doc, syntax);
    expect(tags.map((t) => [t.kind, t.name ?? null])).toEqual([
      ["comment", null],
      ["block", "if"],
      ["end", "if"],
      ["block", "literal"],
      ["end", "literal"],
    ]);
  });

  it("still uses { } for Smarty with HTML, and `{ ` stays text (auto_literal)", () => {
    const html = smartySyntaxWith();
    const doc = "<p>{$title}</p>{* c *}<script>if (a) { b(); }</script>";
    const ranges = templateRanges(doc, html).map((r) => [doc.slice(r.from, r.to), r.style]);
    expect(ranges).toContainEqual(["{", "meta"]);
    expect(ranges).toContainEqual(["$title", "variableName"]);
    expect(ranges).toContainEqual([" c *}", "comment"]);
    expect(ranges).not.toContainEqual(["b", "variableName"]);
  });

  it("works with a custom entry for another data language", () => {
    const sqlSyntax = syntaxWithDelimiters("smarty", { left: "[[", right: "]]" })!;
    const t = tokens(templateParser(sqlSyntax, dataParser("sql")), "SELECT * FROM [[$table]] WHERE x = '{a}'");
    expect(t[0]).toEqual(["SELECT", "keyword"]);
    expect(t).toContainEqual(["[[", "meta"]);
    expect(t).toContainEqual(["$table", "variableName"]);
    expect(t.find(([text]) => text.includes("{a}"))?.[1]).toBe("string");
  });

  it("applies to Go templates too", () => {
    const go = syntaxWithDelimiters("gotemplate", { left: "[[", right: "]]" })!;
    const ranges = templateRanges("{{x}} [[ .Name ]] [[/* c */]]", go).map((r) => r.style);
    expect(ranges).toEqual(["meta", "propertyName", "meta", "comment", "comment"]);
  });
});

describe("Twig with LaTeX and \\VAR{ } / \\BLOCK{ } / \\#{ }", () => {
  const syntax = syntaxWithDelimiters("twig", twigLatex)!;
  const latex = dataParser("latex");
  const doc = [
    "\\#{ invoice }",
    "\\section*{Invoice \\VAR{invoice.number|upper}}",
    "\\BLOCK{for line in lines}",
    "  \\VAR{line.name} & \\amount{\\VAR{line.total}} \\\\",
    "\\BLOCK{endfor}",
  ].join("\n");

  it("keeps LaTeX braces LaTeX and styles only the configured tags", () => {
    const t = tokens(templateParser(syntax, latex), doc);
    expect(t).toContainEqual(["\\VAR{", "meta"]);
    expect(t).toContainEqual(["\\BLOCK{", "meta"]);
    expect(t).toContainEqual(["for", "keyword"]);
    expect(t).toContainEqual(["|upper", "variableName.function"]);
    expect(t).toContainEqual(["\\section", "tag"]);
    expect(t).toContainEqual(["\\amount", "tag"]);
    expect(t.filter(([text, style]) => text === "{" && style === "meta")).toEqual([]);
  });

  it("pairs blocks with their end tags and toggles \\#{ } comments", () => {
    const { tags } = scanTemplate(doc, syntax);
    expect(tags.map((tag) => [tag.kind, tag.name ?? null])).toEqual([
      ["comment", null],
      ["expr", null],
      ["block", "for"],
      ["expr", null],
      ["expr", null],
      ["end", "for"],
    ]);
    let state = EditorState.create({ doc: "\\section{A}", extensions: [templateOverlay(syntax), templateEditing()] });
    toggleComment({ state, dispatch: (tr) => (state = tr.state) });
    expect(state.doc.toString()).toBe("\\#{ \\section{A} }");
    expect(foldable(EditorState.create({ doc, extensions: [templateOverlay(syntax), templateEditing()] }), doc.indexOf("\\BLOCK"), doc.indexOf("}\n  ") + 1)).not.toBeNull();
  });

  it("still uses {{ }} for Twig with HTML", () => {
    const html = syntaxWithDelimiters("twig", {})!;
    const t = templateRanges("<p>{{ name }}</p>{% if a %}{% endif %}", html).map((r) => r.style);
    expect(t).toEqual(["meta", "variableName", "meta", "meta", "keyword", "variableName", "meta", "meta", "keyword", "meta"]);
  });
});

describe("template editing", () => {
  const views: EditorView[] = [];
  afterEach(() => {
    views.splice(0).forEach((v) => v.destroy());
    setLocalSetting(DELIMITERS_KEY, []);
  });

  const stateWith = (doc: string, sel = 0) =>
    EditorState.create({ doc, selection: EditorSelection.cursor(sel), extensions: [templateOverlay(smartySyntaxWith("[[", "]]")), templateEditing()] });

  it("toggles [[* *]] comments", () => {
    let state = stateWith("\\section{A}");
    toggleComment({ state, dispatch: (tr) => (state = tr.state) });
    expect(state.doc.toString()).toBe("[[* \\section{A} *]]");
  });

  it("folds blocks and matches delimiters", () => {
    const doc = "[[foreach $items as $i]]\n  [[$i]]\n[[/foreach]]";
    const state = stateWith(doc);
    expect(foldable(state, 0, state.doc.line(1).to)).toEqual({ from: doc.indexOf("]]") + 2, to: doc.indexOf("[[/") });
    const tags = scanTemplate(doc, state.field(templateSyntaxField)!).tags;
    const end = doc.indexOf("[[/");
    expect(matchingDelimiters(tags, 1)).toEqual([
      [0, 2],
      [doc.indexOf("]]"), doc.indexOf("]]") + 2],
      [end, end + 3],
      [doc.length - 2, doc.length],
    ]);
  });

  it("completes the open block after the closing-tag opener", () => {
    const doc = "[[if $a]][[foreach $b as $c]]x[[/";
    expect(openBlocksAt(scanTemplate(doc, smartySyntaxWith("[[", "]]")).tags, doc.length)).toEqual(["if", "foreach"]);
    const result = templateTagCompletion(new CompletionContext(stateWith(doc, doc.length), doc.length, false));
    expect(result?.options[0]).toMatchObject({ label: "foreach", apply: "foreach]]" });
  });

  /** An editor with `syntax` and closeBrackets, and a function that types like the keyboard. */
  function typingView(syntax: ReturnType<typeof smartySyntaxWith>) {
    const view = new EditorView({
      state: EditorState.create({ extensions: [templateOverlay(syntax), templateEditing(), closeBrackets()] }),
      parent: document.body,
    });
    views.push(view);
    return {
      view,
      type(text: string) {
        for (const ch of text) {
          const { from, to } = view.state.selection.main;
          const handled = view.state.facet(EditorView.inputHandler).some((h) => h(view, from, to, ch, () => view.state.update({ changes: { from, to, insert: ch } })));
          if (!handled) view.dispatch({ changes: { from, to, insert: ch }, selection: { anchor: from + ch.length } });
        }
      },
    };
  }

  it("auto-closes [[ ]] tags and [[* *]] comments", () => {
    const { view, type } = typingView(smartySyntaxWith("[[", "]]"));
    type("[[");
    expect(view.state.doc.toString()).toBe("[[]]");
    type("*");
    expect(view.state.doc.toString()).toBe("[[**]]");
    expect(view.state.selection.main.head).toBe(3);
  });

  it("auto-closes multi-character delimiters that are not brackets", () => {
    const { view, type } = typingView(smartySyntaxWith("<%", "%>"));
    type("<%");
    expect(view.state.doc.toString()).toBe("<%%>");
    type("*");
    expect(view.state.doc.toString()).toBe("<%**%>");
  });

  it("re-highlights open editors when the delimiter settings change", async () => {
    const view = new EditorView({
      state: EditorState.create({ doc: "\\textbf{<%$x%>}", extensions: languageFor("invoice.tex.tpl") }),
      parent: document.body,
    });
    views.push(view);
    expect(view.state.field(templateSyntaxField)?.delimiters[2][0]).toBe("{");
    setLocalSetting(DELIMITERS_KEY, [{ template: "smarty", language: "latex", left: "<%", right: "%>" }]);
    await new Promise((r) => setTimeout(r, 0));
    expect(view.state.field(templateSyntaxField)?.delimiters[2][0]).toBe("<%");
    expect(view.contentDOM.querySelector(".cm-tpl-variableName")?.textContent).toBe("$x");
  });
});
