import { describe, expect, it } from "vitest";
import type { CheckResult, QualityProblem } from "@/ipc/qualityTools";
import { diagnostics } from "@/features/lsp/store";
import { applyResult, problemsOf, toDiagnostic } from "./problems";
import { mockDiff, mockFixedText } from "@/mock/qualityToolsMock";

const problem = (p: Partial<QualityProblem>): QualityProblem => ({
  tool: "phpstan",
  file: "app/A.php",
  line: 2,
  column: 0,
  severity: "error",
  message: "Undefined variable: $x",
  fixable: false,
  ...p,
});

const result = (r: Partial<CheckResult>): CheckResult => ({ files: [], tools: [], problems: [], outcomes: [], ...r });

describe("quality tool problems", () => {
  it("become diagnostics with the tool as source", () => {
    const d = toDiagnostic(problem({ rule: "variable.undefined" }));
    expect(d.source).toBe("PHPStan");
    expect(d.code).toBe("variable.undefined");
    // No end: the whole line.
    expect(d.range).toEqual({ start: { line: 2, character: 0 }, end: { line: 2, character: 100000 } });
    const ranged = toDiagnostic(problem({ tool: "psalm", column: 4, endLine: 2, endColumn: 9 }));
    expect(ranged.range.end).toEqual({ line: 2, character: 9 });
  });

  it("replace the earlier problems of the tools that ran in the checked files", () => {
    applyResult(
      result({
        files: ["app/A.php"],
        tools: ["phpstan", "phpcs"],
        problems: [problem({}), problem({ tool: "phpcs", line: 5, severity: "warning", fixable: true })],
      }),
    );
    expect(problemsOf("app/A.php")).toHaveLength(2);
    // Typing: only phpcs ran again; PHPStan's problem stays.
    applyResult(result({ files: ["app/A.php"], tools: ["phpcs"], problems: [] }));
    expect(problemsOf("app/A.php").map((p) => p.tool)).toEqual(["phpstan"]);
    expect(diagnostics["app/A.php"]?.length).toBe(1);
    // Inspect Code on the project replaces PHPStan's problems everywhere.
    applyResult(result({ tools: ["phpstan"], problems: [problem({ file: "app/B.php" })] }));
    expect(problemsOf("app/A.php")).toHaveLength(0);
    expect(problemsOf("app/B.php")).toHaveLength(1);
  });
});

describe("browser-mode fixers", () => {
  it("fix the demo problems and print a diff", () => {
    const before = "<?php\n\nclass PaymentGateway\n{\n}\n";
    const after = mockFixedText("rector", before);
    expect(after).toContain("final class PaymentGateway");
    expect(mockDiff(before, after)).toBe(
      "--- Original\n+++ New\n@@ -1,5 +1,5 @@\n <?php\n \n-class PaymentGateway\n+final class PaymentGateway\n {\n }\n",
    );
    expect(mockFixedText("phpcs", "<?php  \n$a = 1; \n")).toBe("<?php\n$a = 1;\n");
    expect(mockFixedText("pint", "    use HasFactory, Notifiable;\n")).toBe("    use HasFactory;\n    use Notifiable;\n");
  });
});
