import { describe, expect, it } from "vitest";
import { StringStream, type StreamParser } from "@codemirror/language";
import { goTemplateSyntax, smartySyntaxWith, templateParser, twigSyntax } from "./templates";
import { dataLanguageFromMappings, dataLanguageFromName, DATA_LANGUAGES } from "./templateData";

/** Tokenize a document and return `[text, style]` pairs, skipping whitespace-only tokens. */
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

const latex = DATA_LANGUAGES.find((l) => l.id === "latex")!.parser!;

describe("template data languages", () => {
  it("highlights LaTeX between Twig tags", () => {
    const t = tokens(templateParser(twigSyntax, latex), "\\section{ {{ title|upper }} } % note");
    expect(t).toContainEqual(["\\section", "tag"]);
    expect(t).toContainEqual(["title", "variableName"]);
    expect(t).toContainEqual(["|upper", "variableName.function"]);
    expect(t.find(([text]) => text.startsWith("%"))?.[1]).toBe("comment");
  });

  it("keeps template tags working inside data-language strings", () => {
    const t = tokens(templateParser(twigSyntax, DATA_LANGUAGES.find((l) => l.id === "sql")!.parser!), "SELECT * FROM {{ table }} WHERE id = 1");
    expect(t[0]).toEqual(["SELECT", "keyword"]);
    expect(t).toContainEqual(["table", "variableName"]);
  });

  it("supports custom Smarty delimiters for LaTeX", () => {
    const t = tokens(templateParser(smartySyntaxWith("<{", "}>"), latex), "\\textbf{<{$name}>}");
    expect(t).toContainEqual(["<{", "meta"]);
    expect(t).toContainEqual(["$name", "variableName"]);
    expect(t).toContainEqual(["\\textbf", "tag"]);
  });

  it("detects the data language from compound extensions", () => {
    expect(dataLanguageFromName("templates/invoice.tex.twig")?.id).toBe("latex");
    expect(dataLanguageFromName("reports/top.sql.twig")?.id).toBe("sql");
    expect(dataLanguageFromName("theme/style.css.tpl")?.id).toBe("css");
    expect(dataLanguageFromName("base.html.twig")?.id).toBe("html");
    expect(dataLanguageFromName("cart.tpl")).toBeUndefined();
  });

  it("applies pattern mappings before file names", () => {
    const m = ["legacy/latex/*=latex", "*.mail.twig=text"];
    expect(dataLanguageFromMappings("legacy/latex/letter.tpl", m)?.id).toBe("latex");
    expect(dataLanguageFromMappings("emails/welcome.mail.twig", m)?.id).toBe("text");
    expect(dataLanguageFromMappings("templates/base.html.twig", m)).toBeUndefined();
  });
});

describe("Go templates", () => {
  const go = () => templateParser(goTemplateSyntax);
  const dataLang = (id: string) => DATA_LANGUAGES.find((l) => l.id === id)!.parser!;

  it("styles actions, keywords, builtins, fields and strings", () => {
    const t = tokens(go(), '{{ if eq .Status "paid" }}ok{{ else }}no{{ end }}');
    expect(t).toContainEqual(["{{", "meta"]);
    expect(t).toContainEqual(["if", "keyword"]);
    expect(t).toContainEqual(["eq", "variableName.function"]);
    expect(t).toContainEqual([".Status", "propertyName"]);
    expect(t).toContainEqual(['"paid"', "string"]);
    expect(t).toContainEqual(["else", "keyword"]);
    expect(t).toContainEqual(["end", "keyword"]);
    expect(t.filter(([, s]) => s === "meta")).toHaveLength(6);
  });

  it("keeps trim markers with the delimiters", () => {
    const t = tokens(go(), "{{- range $i, $o := .Items -}}");
    expect(t[0]).toEqual(["{{-", "meta"]);
    expect(t).toContainEqual(["range", "keyword"]);
    expect(t).toContainEqual(["$i", "variableName"]);
    expect(t).toContainEqual([".Items", "propertyName"]);
    expect(t.at(-1)).toEqual(["-}}", "meta"]);
  });

  it("chains selectors and knows nil, true and false", () => {
    const t = tokens(go(), "{{ .User.Name }}{{ if not nil }}{{ true }}{{ end }}");
    expect(t).toContainEqual([".User", "propertyName"]);
    expect(t).toContainEqual([".Name", "propertyName"]);
    expect(t).toContainEqual(["nil", "atom"]);
    expect(t).toContainEqual(["true", "atom"]);
  });

  it("styles pipelines and printf-style calls", () => {
    const t = tokens(go(), '{{ .Total | printf "%.2f" }}');
    expect(t).toContainEqual(["| printf", "variableName.function"]);
    expect(t).toContainEqual(['"%.2f"', "string"]);
  });

  it("handles comments with and without trim markers", () => {
    for (const doc of ["{{/* a note */}}", "{{- /* trimmed note */ -}}"]) {
      const t = tokens(go(), doc);
      expect(t.length).toBeGreaterThan(0);
      expect(t.every(([, s]) => s === "comment")).toBe(true);
    }
    const t = tokens(go(), "{{/* note */}}{{ .X }}");
    expect(t).toContainEqual([".X", "propertyName"]);
  });

  it("keeps raw strings open across }} and lines", () => {
    const t = tokens(go(), "{{ print `a }} b` }}");
    expect(t).toContainEqual(["`a }} b`", "string"]);
    expect(t.at(-1)).toEqual(["}}", "meta"]);
    const multi = tokens(go(), "{{ `line1\nline2` }}after");
    expect(multi).toContainEqual(["`line1", "string"]);
    expect(multi).toContainEqual(["line2`", "string"]);
    expect(multi).toContainEqual(["}}", "meta"]);
  });

  it("continues an action over several lines", () => {
    const t = tokens(go(), "{{ if\n  .Ready\n}}");
    expect(t).toContainEqual(["if", "keyword"]);
    expect(t).toContainEqual([".Ready", "propertyName"]);
    expect(t.at(-1)).toEqual(["}}", "meta"]);
  });

  it("highlights HTML around the actions by default", () => {
    const t = tokens(go(), '<h1 class="{{ .Class }}">{{ .Title }}</h1>');
    expect(t).toContainEqual(["h1", "tagName"]);
    expect(t).toContainEqual([".Class", "propertyName"]);
    expect(t).toContainEqual([".Title", "propertyName"]);
  });

  it("highlights other data languages around the actions", () => {
    const sql = tokens(templateParser(goTemplateSyntax, dataLang("sql")), "SELECT * FROM {{ .Table }} WHERE id = {{ .ID }}");
    expect(sql[0]).toEqual(["SELECT", "keyword"]);
    expect(sql).toContainEqual([".Table", "propertyName"]);
    const yaml = tokens(templateParser(goTemplateSyntax, dataLang("yaml")), "image: {{ .Image }}");
    expect(yaml).toContainEqual([".Image", "propertyName"]);
  });

  it("detects the data language from compound .tmpl names", () => {
    expect(dataLanguageFromName("cmd/main.go.tmpl")?.id).toBe("go");
    expect(dataLanguageFromName("deploy/config.yaml.tmpl")?.id).toBe("yaml");
    expect(dataLanguageFromName("db/query.sql.tmpl")?.id).toBe("sql");
    expect(dataLanguageFromName("views/index.html.tmpl")?.id).toBe("html");
    expect(dataLanguageFromName("deploy.tmpl")).toBeUndefined();
  });
});
