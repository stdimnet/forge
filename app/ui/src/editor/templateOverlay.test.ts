import { afterEach, describe, expect, it } from "vitest";
import { EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { html } from "@codemirror/lang-html";
import { overlayClass, OVERLAY_MAX_CHARS, templateOverlay, templateRanges } from "./templateOverlay";
import { goTemplateSyntax, smartySyntaxWith, twigSyntax } from "./templates";
import { dataLanguageFor, dataLanguageSupport } from "./templateData";
import { languageFor } from "./languages";

/** `[text, style]` of every template token in `doc`. */
const tokens = (doc: string, syntax = goTemplateSyntax) =>
  templateRanges(doc, syntax).map((r) => [doc.slice(r.from, r.to), r.style] as const);

describe("template ranges", () => {
  it("covers only the template tags, never the data language around them", () => {
    const doc = '<h1 class="title">{{ .Title }}</h1>';
    const covered = templateRanges(doc, goTemplateSyntax).map((r) => doc.slice(r.from, r.to)).join("");
    expect(covered).toBe("{{.Title}}");
    expect(tokens(doc)).toContainEqual(["{{", "meta"]);
    expect(tokens(doc)).toContainEqual([".Title", "propertyName"]);
    expect(tokens(doc)).toContainEqual(["}}", "meta"]);
  });

  it("styles keywords, strings, variables and pipelines inside actions", () => {
    const t = tokens('{{ if eq $x "a" }}{{ .Name | printf "%s" }}{{ end }}');
    expect(t).toContainEqual(["if", "keyword"]);
    expect(t).toContainEqual(["eq", "variableName.function"]);
    expect(t).toContainEqual(["$x", "variableName"]);
    expect(t).toContainEqual(['"a"', "string"]);
    expect(t).toContainEqual(["end", "keyword"]);
  });

  it("keeps a tag that spans lines and a raw string that spans lines", () => {
    const doc = "text\n{{ range .Items\n  | printf `a\nb` }}\nmore";
    const t = tokens(doc);
    expect(t.map(([s]) => s).join("")).toContain("range");
    expect(t.some(([s, st]) => st === "string" && s.includes("b"))).toBe(true);
    // The line after the closing delimiter is plain data language again.
    expect(templateRanges(doc, goTemplateSyntax).every((r) => r.to <= doc.indexOf("more"))).toBe(true);
  });

  it("works for other engines and their delimiters", () => {
    expect(tokens("{% if user %}{{ name|upper }}{# note #}{% endif %}", twigSyntax)).toContainEqual(["{#", "comment"]);
    const smarty = tokens("<{$name}>", smartySyntaxWith("<{", "}>"));
    expect(smarty).toContainEqual(["<{", "meta"]);
    expect(smarty).toContainEqual(["$name", "variableName"]);
  });

  it("ignores template-looking text in documents without tags and handles empty lines", () => {
    expect(templateRanges("plain\n\n{ not a tag }\n", goTemplateSyntax)).toEqual([]);
  });
});

describe("template overlay in an editor", () => {
  const views: EditorView[] = [];
  const make = (doc: string, extensions = [html(), templateOverlay(goTemplateSyntax)]) => {
    const view = new EditorView({ state: EditorState.create({ doc, extensions }) });
    views.push(view);
    return view;
  };
  // jsdom has no layout: destroy before CodeMirror's measure pass runs.
  afterEach(() => views.splice(0).forEach((v) => v.destroy()));

  it("marks the tags and keeps the data language's own marks", () => {
    const view = make('<a href="x">{{ if .Ok }}yes{{ end }}</a>');
    const dom = view.contentDOM;
    expect(dom.querySelector(`.${overlayClass("keyword")}`)?.textContent).toBe("if");
    expect(dom.querySelector(".cm-tpl-meta")?.textContent).toBe("{{");
    // The HTML text outside the tags is not decorated by the overlay.
    expect(dom.textContent).toBe('<a href="x">{{ if .Ok }}yes{{ end }}</a>');
    expect([...dom.querySelectorAll(".cm-tpl")].some((el) => el.textContent?.includes("href"))).toBe(false);
  });

  it("follows edits", () => {
    const view = make("<p>hi</p>");
    expect(view.contentDOM.querySelector(".cm-tpl")).toBeNull();
    view.dispatch({ changes: { from: 3, insert: "{{ .Name }}" } });
    expect(view.contentDOM.querySelector(".cm-tpl-propertyName")?.textContent).toBe(".Name");
  });

  it("skips very large documents", () => {
    const view = make("{{ .A }}" + "x".repeat(OVERLAY_MAX_CHARS));
    expect(view.contentDOM.querySelector(".cm-tpl")).toBeNull();
  });
});

describe("template languages use the data language and the overlay", () => {
  it("builds a data language support per compound name", () => {
    expect(dataLanguageFor("index.html.tmpl").id).toBe("html");
    expect(dataLanguageFor("query.sql.tmpl").id).toBe("sql");
    for (const id of ["html", "css", "sql", "yaml", "json", "go", "javascript", "xml", "text"]) {
      const lang = dataLanguageFor(`a.${id === "javascript" ? "js" : id === "text" ? "txt" : id}.tmpl`);
      expect(() => dataLanguageSupport(lang)).not.toThrow();
    }
  });

  it("returns the data language plus the overlay as the file's language", () => {
    const ext = languageFor("views/index.html.tmpl");
    expect(ext).toHaveLength(1);
    expect(Array.isArray(ext[0])).toBe(true);
    expect((ext[0] as unknown[]).length).toBe(2);
  });
});
