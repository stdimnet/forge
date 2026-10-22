import { describe, expect, it } from "vitest";
import type { ConfigurableTemplate } from "@/ipc/filetypes";
import { forgeWasm } from "@/wasm";
import { delimiterSummary, describeProblem, normalizeEntry, pairsOf } from "./TemplateDelimitersEditor";

// The catalog the editor gets from Rust (`filetypes_template_delimiters`).
const catalog = JSON.parse((await forgeWasm()).filetypes_template_delimiters()) as { templates: ConfigurableTemplate[] };
const template = (id: string) => catalog.templates.find((t) => t.id === id);

describe("template delimiters editor", () => {
  const smarty = { template: "smarty", language: "latex", left: "[[", right: "]]" };

  it("summarizes the delimiters of each tag kind", () => {
    expect(delimiterSummary(template("smarty"), smarty)).toBe("[[ ]]");
    expect(delimiterSummary(template("twig"), { template: "twig", language: "latex", variableStart: "\\VAR{", variableEnd: "}" })).toBe("\\VAR{ }   {% %}   {# #}");
  });

  it("keeps only the fields of the template that differ from its defaults", () => {
    expect(normalizeEntry(template("smarty"), { template: "smarty", language: "sql", left: "{", right: " ]] ", blockStart: "x" })).toEqual({
      template: "smarty",
      language: "sql",
      right: "]]",
    });
  });

  it("shows left/right as one pair and the tag kinds of Twig", () => {
    expect(pairsOf(template("smarty")).map(([s, e]) => [s.label, e.key])).toEqual([["Delimiters start", "right"]]);
    expect(pairsOf(template("twig")).map(([s]) => s.label)).toEqual(["Variable start", "Block start", "Comment start"]);
  });

  it("words a duplicate pair for the form", () => {
    expect(describeProblem("duplicate entry for smarty + latex", "Smarty", "LaTeX")).toBe("Smarty in LaTeX already has delimiters.");
    expect(describeProblem("every entry needs a `language`", "Smarty", "LaTeX")).toBe("every entry needs a `language`");
  });
});
