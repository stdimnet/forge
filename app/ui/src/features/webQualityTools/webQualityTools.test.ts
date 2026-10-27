import { describe, expect, it } from "vitest";
import type { WebCheckResult, WebProblem } from "@/ipc/webQualityTools";
import { diagnostics } from "@/features/lsp/store";
import { applyResult, problemsOf, toDiagnostic } from "./problems";
import { isWebFile } from "./run";
import { mockFormat, mockWebFixed, mockWebProblems } from "@/mock/webQualityToolsMock";

const problem = (p: Partial<WebProblem>): WebProblem => ({
  tool: "eslint",
  file: "src/a.ts",
  line: 3,
  column: 2,
  endLine: 3,
  endColumn: 5,
  severity: "error",
  message: "Unexpected var, use let or const instead.",
  rule: "no-var",
  fixable: true,
  ...p,
});

const result = (r: Partial<WebCheckResult>): WebCheckResult => ({ files: [], tools: [], problems: [], outcomes: [], ...r });

describe("web quality tool problems", () => {
  it("become diagnostics named after the tool and rule", () => {
    const d = toDiagnostic(problem({}));
    expect(d.source).toBe("ESLint");
    expect(d.message).toBe("ESLint: Unexpected var, use let or const instead. (no-var)");
    expect(d.range).toEqual({ start: { line: 3, character: 2 }, end: { line: 3, character: 5 } });
    // Without an end: one character.
    const point = toDiagnostic(problem({ tool: "stylelint", endLine: undefined, endColumn: undefined, rule: undefined }));
    expect(point.range.end).toEqual({ line: 3, character: 3 });
  });

  it("are replaced per tool by the next check of the file", () => {
    applyResult(result({ files: ["src/b.ts"], tools: ["eslint", "biome"], problems: [problem({ file: "src/b.ts" }), problem({ file: "src/b.ts", tool: "biome" })] }));
    expect(problemsOf("src/b.ts")).toHaveLength(2);
    applyResult(result({ files: ["src/b.ts"], tools: ["eslint"], problems: [] }));
    expect(problemsOf("src/b.ts").map((p) => p.tool)).toEqual(["biome"]);
    expect(diagnostics["src/b.ts"]?.map((d) => d.source)).toContain("Biome");
  });

  it("checks only web files", () => {
    expect(isWebFile("src/App.tsx")).toBe(true);
    expect(isWebFile("src/app/card.scss")).toBe(true);
    expect(isWebFile("app/Models/User.php")).toBe(false);
    expect(isWebFile("node_modules/x/index.js")).toBe(false);
  });
});

describe("browser-mode tools", () => {
  const ts = "export function f(items: number[]) {\n  var total = 0;\n  if (items.length == 0) return total;\n  let x = 1;\n  return x!.toFixed();\n}\n";

  it("find the demo problems and fix them like the tools", () => {
    expect(mockWebProblems("eslint", "a.ts", ts).map((p) => [p.rule, p.line, p.column])).toEqual([
      ["no-var", 1, 2],
      ["eqeqeq", 2, 19],
    ]);
    expect(mockWebFixed("eslint", "a.ts", ts)).toContain("  let total = 0;");
    const biome = mockWebProblems("biome", "a.ts", ts).map((p) => p.rule);
    expect(biome).toEqual(["lint/suspicious/noDoubleEquals", "lint/style/useConst", "lint/style/noNonNullAssertion"]);
    const fixed = mockWebFixed("biome", "a.ts", ts);
    expect(fixed).toContain("items.length === 0");
    expect(fixed).toContain("  const x = 1;");
    // `var` is not one of the demo Biome rules.
    expect(fixed).toContain("var total");
  });

  it("fix Stylelint problems in SCSS", () => {
    const scss = ".card {\n  color: #ffg;\n  background: #ffffff;\n  margin: 0px;\n}\n";
    expect(mockWebProblems("stylelint", "a.scss", scss).map((p) => p.rule)).toEqual([
      "color-no-invalid-hex",
      "color-hex-length",
      "length-zero-no-unit",
    ]);
    expect(mockWebFixed("stylelint", "a.scss", scss)).toBe(".card {\n  color: #ffg;\n  background: #fff;\n  margin: 0;\n}\n");
  });

  it("formats like Prettier", () => {
    expect(mockFormat("const f = ( a: number )  =>  a ;  \n\n\n\nf(1);\n")).toBe("const f = (a: number) => a;\n\nf(1);\n");
    expect(mockFormat('<div className="card">x</div>\n')).toBe('<div className="card">x</div>\n');
  });
});
