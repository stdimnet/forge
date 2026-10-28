import { describe, expect, it } from "vitest";
import { Text } from "@codemirror/state";
import { demoFiles } from "./demoProject";
import {
  completionAt,
  definitionAt,
  diagnosticsFor,
  formatEdits,
  hoverAt,
  offsetAt,
  positionAt,
  signatureAt,
  symbolsOf,
} from "./lspMock";
import { countBySeverity, filesWithProblems, sortDiagnostics } from "@/features/lsp/store";
import { fromLsp, toLsp } from "@/features/lsp/util";

const USER = "app/Models/User.php";
const INVOICE = "app/Services/InvoiceService.php";
const posOf = (path: string, needle: string, delta = 0) =>
  positionAt(demoFiles[path], demoFiles[path].indexOf(needle) + delta);

describe("mock language server", () => {
  it("answers hovers with php-lsp's (Intelephense-style) markdown", () => {
    const hover = hoverAt(USER, posOf(USER, "hasMany(", 2))!;
    expect(hover.contents).toContain("HasRelationships::hasMany");
    expect(hover.contents).toContain("_@param_");
    expect(hoverAt(USER, posOf(USER, "extends Authenticatable", 10))!.contents).toContain("Illuminate\\Foundation\\Auth\\User");
    expect(hoverAt(INVOICE, posOf(INVOICE, "array_map", 3))!.contents).toContain("php.net");
  });

  it("reports diagnostics for the demo file and none for clean files", () => {
    const diags = diagnosticsFor(INVOICE, demoFiles[INVOICE]);
    const counts = countBySeverity(diags);
    expect(counts.error).toBe(1);
    expect(counts.warning).toBe(1);
    const error = sortDiagnostics(diags)[0];
    expect(error.message).toContain("markAsPaid");
    expect(error.range.start).toEqual(posOf(INVOICE, "markAsPaid"));
    expect(diagnosticsFor(USER, demoFiles[USER])).toEqual([]);
  });

  it("completes members after -> and variables after $", () => {
    const text = demoFiles[USER];
    const at = text.indexOf("$this->hasMany") + "$this->".length;
    const labels = completionAt(USER, positionAt(text, at)).items.map((i) => i.label);
    expect(labels).toContain("hasMany");
    const vars = completionAt(USER, positionAt(text, text.indexOf("$this->hasMany") + 1)).items;
    expect(vars.every((i) => i.kind === "variable")).toBe(true);
  });

  it("finds the active parameter of a call", () => {
    const help = signatureAt(INVOICE, posOf(INVOICE, "$order->lines)"))!;
    expect(help.signatures[0].label).toContain("array_map(");
    expect(help.activeParameter).toBe(1);
    const [s, e] = help.signatures[0].parameters[1].label;
    expect(help.signatures[0].label.slice(s, e)).toBe("array $array");
  });

  it("goes to declarations and lists symbols", () => {
    const [loc] = definitionAt(INVOICE, posOf(INVOICE, "Order $order", 1));
    expect(loc.path).toBe("app/Models/Order.php");
    const syms = symbolsOf(USER);
    expect(syms[0].name).toBe("User");
    expect(syms[0].children.map((c) => c.name)).toContain("orders");
  });

  it("formats trailing whitespace and tabs", () => {
    const [edit] = formatEdits("<?php\t\n\n\n\n\techo 1;  ");
    expect(edit.newText).toBe("<?php\n\n    echo 1;\n");
    expect(formatEdits("<?php\n")).toEqual([]);
  });

  it("converts positions consistently", () => {
    const text = "ab\ncdé\n";
    expect(offsetAt(text, positionAt(text, 5))).toBe(5);
    const doc = Text.of(text.split("\n"));
    expect(toLsp(doc, 5)).toEqual({ line: 1, character: 2 });
    expect(fromLsp(doc, { line: 1, character: 99 })).toBe(6);
    expect(fromLsp(doc, { line: 99, character: 0 })).toBe(doc.length);
  });
});

describe("problem helpers", () => {
  it("orders files with errors first", () => {
    const d = (severity: "error" | "weak") => ({
      range: { start: { line: 0, character: 0 }, end: { line: 0, character: 1 } },
      severity,
      message: severity,
    });
    expect(filesWithProblems({ "b.php": [d("error")], "a.php": [d("weak")], "c.php": [] })).toEqual(["b.php", "a.php"]);
  });
});
