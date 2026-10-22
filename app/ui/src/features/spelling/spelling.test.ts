import { describe, expect, it } from "vitest";
import { EditorState } from "@codemirror/state";
import { php } from "@codemirror/lang-php";
import { markdown } from "@codemirror/lang-markdown";
import { invoke } from "@/ipc";
import type { EditorDiagnostic } from "@/ipc/lsp";
import type { ContextAction } from "@/ipc/codeintel";
import type { SpellProblem } from "@/ipc/spell";
import { diagnostics, publishDiagnostics } from "@/features/lsp/store";
import { collectSegments, expandRange, mergeRanges } from "./segments";
import { spellOptions } from "./options";
import { toDiagnostics } from "./inspector";
import "@/mock/spellMock";

const PHP = `<?php
/**
 * Recieve the order.
 */
function sendInvoce(string $ordr): string
{
    // the the total
    return sprintf('Totl: %s', strtolower($ordr));
}
`;

const segmentsOf = (doc: string, prose = false, extensions = [php()]) => {
  const state = EditorState.create({ doc, extensions });
  return collectSegments(state, [[0, doc.length]], prose);
};

describe("spelling segments", () => {
  it("finds comments, doc comments, strings and declared names in code", () => {
    const segments = segmentsOf(PHP).map((s) => [s.kind, s.text]);
    expect(segments).toEqual([
      ["docComment", "/**\n * Recieve the order.\n */"],
      ["identifier", "sendInvoce"],
      ["identifier", "$ordr"],
      ["comment", "// the the total"],
      ["string", "'Totl: %s'"],
      ["identifier", "$ordr"],
    ]);
  });

  it("takes the prose of Markdown without code and links", () => {
    const md = "# Instalation\n\nRun `pnpm instal` and see [the docs](https://exmple.com).\n\n```sh\nnpm runn dev\n```\n";
    const text = segmentsOf(md, true, [markdown()]).map((s) => s.text).join("|");
    expect(text).toContain("Instalation");
    expect(text).toContain("the docs");
    expect(text).not.toContain("instal`");
    expect(text).not.toContain("exmple");
    expect(text).not.toContain("runn");
  });

  it("grows changed ranges to whole comments", () => {
    const state = EditorState.create({ doc: PHP, extensions: [php()] });
    const at = PHP.indexOf("Recieve");
    expect(expandRange(state, at, at)).toEqual([PHP.indexOf("/**"), PHP.indexOf("*/") + 2]);
    expect(mergeRanges([[5, 9], [0, 3], [2, 4]])).toEqual([[0, 4], [5, 9]]);
  });
});

describe("spelling inspection (Rust through WebAssembly)", () => {
  it("reports typos and grammar with fixes", async () => {
    const segments = segmentsOf(PHP);
    const problems = await invoke<SpellProblem[]>("spell_inspect", { request: { segments, options: spellOptions(), words: [] } });
    const found = problems.map((p) => [p.kind, PHP.slice(p.from, p.to)]);
    expect(found).toEqual([
      ["typo", "Recieve"],
      ["typo", "Invoce"],
      ["typo", "ordr"],
      ["grammar", "the"],
      ["typo", "Totl"],
      ["typo", "ordr"],
    ]);

    const recieve = problems[0];
    const actions = await invoke<ContextAction[]>("spell_context_actions", {
      request: { problems: [recieve], words: [], hasProject: true },
    });
    expect(actions.map((a) => a.title)).toEqual([
      "Typo: Change to...",
      "Save 'recieve' to project-level dictionary",
      "Save 'recieve' to application-level dictionary",
    ]);
    expect((actions[0].data as { suggestions: string[] }).suggestions[0]).toBe("Receive");

    const rename = await invoke<ContextAction[]>("spell_context_actions", {
      request: { problems: [problems[1]], words: [], hasProject: false },
    });
    expect(rename[0].title).toBe("Typo: Rename to...");
    expect((rename[0].data as { suggestions: string[] }).suggestions).toContain("sendInvoice");

    const accepted = await invoke<SpellProblem[]>("spell_inspect", { request: { segments, options: spellOptions(), words: ["recieve", "invoce", "ordr", "totl"] } });
    expect(accepted.map((p) => p.kind)).toEqual(["grammar"]);
  });

  it("publishes problems next to language server diagnostics", () => {
    const path = "app/spelling-test.php";
    const lsp: EditorDiagnostic = { range: { start: { line: 0, character: 0 }, end: { line: 0, character: 5 } }, severity: "error", message: "Syntax error" };
    publishDiagnostics(path, [lsp]);
    const state = EditorState.create({ doc: PHP });
    const typo: SpellProblem = { from: 8, to: 15, kind: "typo", context: "docComment", message: "Typo: In word 'Recieve'", word: "Recieve" };
    publishDiagnostics(path, toDiagnostics(state, [typo]), "spelling");
    expect(diagnostics[path].map((d) => [d.severity, d.message])).toEqual([
      ["error", "Syntax error"],
      ["typo", "Typo: In word 'Recieve'"],
    ]);
    publishDiagnostics(path, []);
    expect(diagnostics[path].map((d) => d.severity)).toEqual(["typo"]);
  });
});
