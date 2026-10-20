import { describe, expect, it } from "vitest";
import { delimiterSummary, duplicateOf, normalizeEntry } from "./TemplateDelimitersEditor";

describe("template delimiters editor", () => {
  const smarty = { template: "smarty", language: "latex", left: "[[", right: "]]" };

  it("summarizes the delimiters of each tag kind", () => {
    expect(delimiterSummary(smarty)).toBe("[[ ]]");
    expect(delimiterSummary({ template: "twig", language: "latex", variableStart: "\\VAR{", variableEnd: "}" })).toBe("\\VAR{ }   {% %}   {# #}");
  });

  it("keeps only the fields of the template that differ from its defaults", () => {
    expect(normalizeEntry({ template: "smarty", language: "sql", left: "{", right: " ]] ", blockStart: "x" })).toEqual({
      template: "smarty",
      language: "sql",
      right: "]]",
    });
  });

  it("finds another entry with the same pair", () => {
    const entries = [smarty, { template: "twig", language: "latex" }];
    expect(duplicateOf(entries, { template: "smarty", language: "latex" }, null)).toBe(0);
    expect(duplicateOf(entries, smarty, 0)).toBe(-1);
    expect(duplicateOf(entries, { template: "smarty", language: "html" }, null)).toBe(-1);
  });
});
