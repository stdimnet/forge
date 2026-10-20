import { describe, expect, it } from "vitest";
import { StringStream, type StreamParser } from "@codemirror/language";
import { smartySyntaxWith, templateParser, twigSyntax } from "./templates";
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
