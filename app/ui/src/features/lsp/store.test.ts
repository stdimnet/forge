import { describe, expect, it } from "vitest";
import type { EditorDiagnostic } from "@/ipc/lsp";
import { PHP_INSPECTIONS_SOURCE, withoutInspectionTwins } from "./store";

const at = (line: number, from: number, to: number) => ({ start: { line, character: from }, end: { line, character: to } });
const diag = (d: Partial<EditorDiagnostic>): EditorDiagnostic => ({ range: at(3, 8, 20), severity: "weak", message: "", ...d });

describe("PHP inspections next to the language server", () => {
  const server = [
    diag({ message: "'payNow' is deprecated.", deprecated: true, source: "php-lsp" }),
    diag({ range: at(9, 8, 30), message: "Unreachable code.", unnecessary: true, source: "php-lsp" }),
  ];

  it("drops the inspection's twin of a server problem", () => {
    const own = [
      diag({ range: at(3, 10, 16), message: "Method 'payNow' is deprecated", deprecated: true, code: "PhpDeprecation" }),
      diag({ range: at(9, 8, 30), message: "Unreachable statement", unnecessary: true, code: "PhpUnreachableStatement" }),
      diag({ range: at(5, 0, 4), message: "Expected values", code: "PhpExpectedValuesShouldBeUsed" }),
    ];
    expect(withoutInspectionTwins(PHP_INSPECTIONS_SOURCE, own, server).map((d) => d.code)).toEqual(["PhpExpectedValuesShouldBeUsed"]);
  });

  it("keeps everything without a server or for other sources", () => {
    const own = [diag({ deprecated: true, code: "PhpDeprecation" })];
    expect(withoutInspectionTwins(PHP_INSPECTIONS_SOURCE, own, [])).toEqual(own);
    expect(withoutInspectionTwins("spell", own, server)).toEqual(own);
    // A deprecation elsewhere on the line is not a twin.
    const elsewhere = [diag({ range: at(3, 30, 40), deprecated: true, code: "PhpDeprecation" })];
    expect(withoutInspectionTwins(PHP_INSPECTIONS_SOURCE, elsewhere, server)).toEqual(elsewhere);
  });
});
