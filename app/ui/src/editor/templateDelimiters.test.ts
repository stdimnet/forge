import { afterEach, describe, expect, it } from "vitest";
import { StringStream, foldable, type StreamParser } from "@codemirror/language";
import { EditorSelection, EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { toggleComment } from "@codemirror/commands";
import { CompletionContext } from "@codemirror/autocomplete";
import builtinPlugins from "@/mock/builtinPlugins.json";
import { setLocalSetting } from "@/state/settings";
import { smartySyntaxWith, templateParser } from "./templates";
import { DATA_LANGUAGES } from "./templateData";
import { CONFIGURABLE_TEMPLATES, delimitersFor, parseDelimiterRule, resolveDelimiters } from "./templateDelimiters";
import { scanTemplate, templateOverlay, templateRanges, templateSyntaxField } from "./templateOverlay";
import { matchingDelimiters, openBlocksAt, templateEditing, templateTagCompletion } from "./templateEditing";
import { languageFor } from "./languages";

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
const smarty = CONFIGURABLE_TEMPLATES.smarty;

/** The default value of a setting in the built-in manifests (browser snapshot). */
function manifestDefault(key: string): unknown {
  for (const plugin of builtinPlugins as { contributions?: { settings?: { key: string; default?: unknown }[] } }[]) {
    const spec = plugin.contributions?.settings?.find((s) => s.key === key);
    if (spec) return spec.default;
  }
  return undefined;
}

describe("delimiter resolution", () => {
  const defaults = { left: "{", right: "}" };

  it("parses override rules and skips malformed ones", () => {
    expect(parseDelimiterRule("latex=<< >>")).toEqual({ target: "latex", left: "<<", right: ">>" });
    expect(parseDelimiterRule(" legacy/pdf/*.tpl = [[  ]] ")).toEqual({ target: "legacy/pdf/*.tpl", left: "[[", right: "]]" });
    expect(parseDelimiterRule("css={= =}")).toEqual({ target: "css", left: "{=", right: "=}" });
    expect(parseDelimiterRule("latex=<<")).toBeUndefined();
    expect(parseDelimiterRule("=<< >>")).toBeUndefined();
  });

  it("uses the language defaults without settings", () => {
    expect(resolveDelimiters(defaults, {}, "cart.tpl", "html")).toEqual(defaults);
    expect(resolveDelimiters(defaults, { left: "", right: " " }, "cart.tpl", "html")).toEqual(defaults);
  });

  it("applies the rule of the file's data language", () => {
    const settings = { overrides: ["latex=<< >>"] };
    expect(resolveDelimiters(defaults, settings, "invoice.tex.tpl", "latex")).toEqual({ left: "<<", right: ">>" });
    expect(resolveDelimiters(defaults, settings, "cart.tpl", "html")).toEqual(defaults);
  });

  it("lets user settings override the defaults, and file patterns win over data languages", () => {
    const settings = { left: "<{", right: "}>", overrides: ["latex=<< >>", "legacy/pdf/*.tpl=[[ ]]"] };
    expect(resolveDelimiters(defaults, settings, "cart.tpl", "html")).toEqual({ left: "<{", right: "}>" });
    expect(resolveDelimiters(defaults, settings, "letter.tex.tpl", "latex")).toEqual({ left: "<<", right: ">>" });
    expect(resolveDelimiters(defaults, settings, "legacy/pdf/letter.tex.tpl", "latex")).toEqual({ left: "[[", right: "]]" });
  });

  it("ships the Smarty + LaTeX rule as manifest data, not as code", () => {
    expect(manifestDefault("templates.smarty.delimiterOverrides")).toEqual(["latex=<< >>"]);
    expect(manifestDefault("templates.smarty.leftDelimiter")).toBe("{");
    // Without settings (no schema loaded) LaTeX gets no special treatment.
    expect(delimitersFor(smarty, "invoice.tex.tpl")).toEqual({ left: "{", right: "}" });
  });
});

describe("Smarty with LaTeX and << >>", () => {
  const syntax = smartySyntaxWith("<<", ">>");
  const latex = dataParser("latex");

  it("keeps LaTeX braces LaTeX and styles only << >> tags", () => {
    const t = tokens(templateParser(syntax, latex), "\\textbf{<<$name|upper>>} \\amount{#1}");
    expect(t).toContainEqual(["\\textbf", "tag"]);
    expect(t).toContainEqual(["<<", "meta"]);
    expect(t).toContainEqual(["$name", "variableName"]);
    expect(t).toContainEqual(["|upper", "variableName.function"]);
    expect(t).toContainEqual([">>", "meta"]);
    // The braces belong to LaTeX, not to Smarty.
    const braces = t.filter(([text]) => text === "{" || text === "}");
    expect(braces.length).toBe(4);
    expect(braces.every(([, style]) => style !== "meta")).toBe(true);
  });

  it("does not let a `%` inside a tag start a LaTeX comment", () => {
    const t = tokens(templateParser(syntax, latex), '\\amount{<<$total|string_format:"%.2f">>} \\\\ % note');
    expect(t).toContainEqual(['"%.2f"', "string"]);
    expect(t.find(([text]) => text === "}")?.[1]).not.toBe("comment");
    expect(t[t.length - 1]).toEqual(["% note", "comment"]);
  });

  it("follows the delimiters for comments, closing tags and literal blocks", () => {
    const doc = "<<* note {x} *>>\n<<if $a>>x<</if>>\n<<literal>><<$no>><</literal>>";
    const ranges = templateRanges(doc, syntax).map((r) => [doc.slice(r.from, r.to), r.style]);
    expect(ranges).toContainEqual([" note {x} *>>", "comment"]);
    expect(ranges).toContainEqual(["if", "keyword"]);
    expect(ranges).toContainEqual(["<</", "meta"]);
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

  it("works with a custom rule for another data language", () => {
    const d = resolveDelimiters(smarty.defaults, { overrides: ["sql=[[ ]]"] }, "report.sql.tpl", "sql");
    const t = tokens(templateParser(smarty.syntax(d), dataParser("sql")), "SELECT * FROM [[$table]] WHERE x = '{a}'");
    expect(t[0]).toEqual(["SELECT", "keyword"]);
    expect(t).toContainEqual(["[[", "meta"]);
    expect(t).toContainEqual(["$table", "variableName"]);
    expect(t.find(([text]) => text.includes("{a}"))?.[1]).toBe("string");
  });

  it("applies to Go templates too", () => {
    const go = CONFIGURABLE_TEMPLATES.gotemplate;
    const d = resolveDelimiters(go.defaults, { overrides: ["latex=[[ ]]"] }, "doc.tex.tmpl", "latex");
    const ranges = templateRanges("{{x}} [[ .Name ]] [[/* c */]]", go.syntax(d)).map((r) => r.style);
    expect(ranges).toEqual(["meta", "propertyName", "meta", "comment", "comment"]);
  });
});

describe("template editing", () => {
  const views: EditorView[] = [];
  afterEach(() => {
    views.splice(0).forEach((v) => v.destroy());
    setLocalSetting("templates.smarty.delimiterOverrides", []);
  });

  const stateWith = (doc: string, sel = 0) =>
    EditorState.create({ doc, selection: EditorSelection.cursor(sel), extensions: [templateOverlay(smartySyntaxWith("<<", ">>")), templateEditing()] });

  it("toggles <<* *>> comments", () => {
    let state = stateWith("\\section{A}");
    toggleComment({ state, dispatch: (tr) => (state = tr.state) });
    expect(state.doc.toString()).toBe("<<* \\section{A} *>>");
  });

  it("folds blocks and matches delimiters", () => {
    const doc = "<<foreach $items as $i>>\n  <<$i>>\n<</foreach>>";
    const state = stateWith(doc);
    expect(foldable(state, 0, state.doc.line(1).to)).toEqual({ from: doc.indexOf(">>") + 2, to: doc.indexOf("<</") });
    const tags = scanTemplate(doc, state.field(templateSyntaxField)!).tags;
    const end = doc.indexOf("<</");
    expect(matchingDelimiters(tags, 1)).toEqual([
      [0, 2],
      [doc.indexOf(">>"), doc.indexOf(">>") + 2],
      [end, end + 3],
      [doc.length - 2, doc.length],
    ]);
  });

  it("completes the open block after the closing-tag opener", () => {
    const doc = "<<if $a>><<foreach $b as $c>>x<</";
    expect(openBlocksAt(scanTemplate(doc, smartySyntaxWith("<<", ">>")).tags, doc.length)).toEqual(["if", "foreach"]);
    const result = templateTagCompletion(new CompletionContext(stateWith(doc, doc.length), doc.length, false));
    expect(result?.options[0]).toMatchObject({ label: "foreach", apply: "foreach>>" });
  });

  it("auto-closes multi-character delimiters and comments", () => {
    const view = new EditorView({ state: stateWith("<"), parent: document.body });
    views.push(view);
    view.dispatch({ selection: { anchor: 1 } });
    const type = (text: string) => {
      const { from, to } = view.state.selection.main;
      const handled = view.state.facet(EditorView.inputHandler).some((h) => h(view, from, to, text, () => view.state.update({ changes: { from, to, insert: text } })));
      if (!handled) view.dispatch({ changes: { from, to, insert: text }, selection: { anchor: from + text.length } });
    };
    type("<");
    expect(view.state.doc.toString()).toBe("<<>>");
    type("*");
    expect(view.state.doc.toString()).toBe("<<**>>");
    expect(view.state.selection.main.head).toBe(3);
  });

  it("re-highlights open editors when the delimiter settings change", async () => {
    const view = new EditorView({
      state: EditorState.create({ doc: "\\textbf{[[$x]]}", extensions: languageFor("invoice.tex.tpl") }),
      parent: document.body,
    });
    views.push(view);
    expect(view.state.field(templateSyntaxField)?.delimiters[2][0]).toBe("{");
    setLocalSetting("templates.smarty.delimiterOverrides", ["latex=[[ ]]"]);
    await new Promise((r) => setTimeout(r, 0));
    expect(view.state.field(templateSyntaxField)?.delimiters[2][0]).toBe("[[");
    expect(view.contentDOM.querySelector(".cm-tpl-variableName")?.textContent).toBe("$x");
  });
});
