/**
 * Browser-mode coverage: Run with Coverage runs of the demos end with a canned report
 * (PHPUnit / Pest: Clover XML of the shop's `app/` classes; busted: LCOV of the Lua game's
 * `src/` and `lib/`), read by the Rust `forge-testing` crate through WebAssembly like in the
 * desktop app. Suites live in memory; nothing is written.
 */
import { registerMock } from "@/ipc";
import type { CoverageSuite, CoverageView } from "@/ipc/testing";
import { callWasmSync, forgeWasm } from "@/wasm";
import { mockFiles } from "./workspaceMock";

interface Suite {
  info: CoverageSuite;
  report: unknown;
  project: string;
}

const suites: Suite[] = [];
let seq = 0;

/** A stable pseudo-random number in [0, 1) for a path and a line. */
function noise(path: string, line: number): number {
  let h = 2166136261;
  for (const c of `${path}:${line}`) h = Math.imul(h ^ c.charCodeAt(0), 16777619);
  return ((h >>> 0) % 1000) / 1000;
}

/** Lines that hold statements (not blank, comments, braces, imports or declarations). */
function statementLines(text: string, comment: RegExp, skip: RegExp): number[] {
  const out: number[] = [];
  text.split("\n").forEach((l, i) => {
    const t = l.trim();
    if (!t || comment.test(t) || skip.test(t)) return;
    out.push(i + 1);
  });
  return out;
}

/** Whole files of the demo not covered at all (a realistic report has some). */
const UNTESTED = /PaymentGateway|SalesReport|PaymentController/;

/** Clover XML of the PHP demo's `app/` classes, as PHPUnit `--coverage-clover` writes it. */
export function demoClover(root: string): string {
  const files = Object.keys(mockFiles).filter((p) => p.startsWith("app/") && p.endsWith(".php"));
  const xml = files
    .map((p) => {
      const lines = statementLines(mockFiles[p], /^(\/\/|\*|\/\*|#)/, /^(<\?php|namespace|use |declare|[{}]|\);?$|\];?$|class |final class|abstract class|enum |interface |trait |\/\*\*)/);
      const untested = UNTESTED.test(p);
      const rows = lines
        .map((n) => {
          const r = noise(p, n);
          const t = mockFiles[p].split("\n")[n - 1].trim();
          if (/^(public|protected|private)?\s*(static\s+)?function /.test(t)) {
            return `      <line num="${n}" type="method" name="m${n}" visibility="public" complexity="1" crap="1" count="${untested || r < 0.15 ? 0 : 1 + Math.floor(r * 9)}"/>`;
          }
          if (/^(if|elseif|\} elseif|while|foreach|for|match|return .*\?)/.test(t) || t.includes(" ?? ") || t.includes(" ? ")) {
            const tc = untested ? 0 : r < 0.8 ? 1 + Math.floor(r * 4) : 0;
            const fc = untested ? 0 : r < 0.45 ? 1 : 0;
            return `      <line num="${n}" type="cond" truecount="${tc}" falsecount="${fc}"/>`;
          }
          return `      <line num="${n}" type="stmt" count="${untested || r < 0.18 ? 0 : 1 + Math.floor(r * 12)}"/>`;
        })
        .join("\n");
      return `    <file name="${root}/${p}">\n${rows}\n    </file>`;
    })
    .join("\n");
  return `<?xml version="1.0" encoding="UTF-8"?>\n<coverage generated="1761472800">\n  <project timestamp="1761472800">\n${xml}\n  </project>\n</coverage>\n`;
}

/** LCOV of the Lua demo's `src/` and `lib/`, as luacov's LCOV reporter writes it (paths relative to the working dir). */
export function demoLcov(): string {
  const files = Object.keys(mockFiles).filter((p) => /^(src|lib)\/.*\.lua$/.test(p));
  return files
    .map((p) => {
      const lines = statementLines(mockFiles[p], /^--/, /^(end\)?,?$|local \w+ = \{\}$|return \w+$|\}$)/);
      const untested = /hud|world/.test(p);
      const da = lines.map((n) => `DA:${n},${untested ? 0 : noise(p, n) < 0.2 ? 0 : 1 + Math.floor(noise(p, n) * 6)}`).join("\n");
      return `TN:\nSF:${p}\n${da}\nend_of_record`;
    })
    .join("\n");
}

/** Add a suite from a report text (paths resolved against `root` / `cwd` and `mappings`). */
export async function addMockCoverage(
  name: string,
  text: string,
  opts: { root: string; project: string; cwd?: string; mappings?: { remote: string; local: string }[]; rules?: { pattern: string; replacement: string }[] },
): Promise<CoverageSuite> {
  const wasm = await forgeWasm();
  const parsed = callWasmSync<{ report: { files: unknown[] }; format: string; lines: CoverageSuite["lines"]; branches: CoverageSuite["branches"]; lineText: string; branchText: string }>(
    wasm.coverage_parse,
    { text, root: opts.root, cwd: opts.cwd ?? null, mappings: opts.mappings ?? [], rules: opts.rules ?? [] },
  );
  const info: CoverageSuite = {
    id: `coverage-${++seq}`,
    name,
    at: Date.now(),
    format: parsed.format,
    files: parsed.report.files.length,
    lines: parsed.lines,
    branches: parsed.branches,
    lineText: parsed.lineText,
    branchText: parsed.branchText,
  };
  suites.unshift({ info, report: parsed.report, project: opts.project });
  suites.splice(10);
  return info;
}

const find = (id: unknown) => (id ? suites.find((s) => s.info.id === id) : suites[0]);

registerMock("coverage_suites", () => suites.map((s) => s.info));
registerMock("coverage_view", async ({ id }): Promise<CoverageView | null> => {
  const s = find(id);
  if (!s) return null;
  const wasm = await forgeWasm();
  return { suite: s.info, nodes: callWasmSync(wasm.coverage_summary, { report: s.report, project: s.project }) };
});
registerMock("coverage_file", async ({ path, id }) => {
  const s = find(id);
  if (!s) return [];
  return callWasmSync((await forgeWasm()).coverage_marks, { report: s.report, path });
});
registerMock("coverage_remove", ({ id }) => {
  const i = suites.findIndex((s) => s.info.id === id);
  if (i >= 0) suites.splice(i, 1);
});
registerMock("coverage_import", async ({ path }) => {
  const text = mockFiles[String(path)];
  if (text === undefined) throw new Error(`${String(path)}: no such file`);
  const { projectRoot } = await import("./runMock");
  const root = projectRoot();
  return addMockCoverage(String(path).split("/").pop() ?? String(path), text, { root, project: root.split("/").pop() ?? "" });
});
