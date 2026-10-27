/**
 * Browser-mode coverage: Run with Coverage runs of the demos end with a canned report built
 * from the demo's files, read by the Rust `forge-testing` crate through WebAssembly like in
 * the desktop app:
 *
 * - PHP test frameworks: Clover XML of the shop's `app/` classes, plus PHPUnit's XML report
 *   with the tests of each line when the configuration tracks per-test coverage;
 * - busted: LCOV of the Lua game's `src/` and `lib/`;
 * - Vitest / Jest: Istanbul JSON (`coverage-final.json`) with statements, branches and
 *   functions; Mocha, node --test and Node.js configurations: LCOV (c8 / nyc);
 * - Go, Cargo and other manifest frameworks: LCOV of the project's sources.
 *
 * Suites live in memory; Show Coverage Data merges them and Generate Coverage Report renders
 * HTML through WebAssembly too. Nothing is written.
 */
import { registerMock } from "@/ipc";
import type { CoverageSuite, CoverageView } from "@/ipc/testing";
import type { RunConfiguration } from "@/ipc/run";
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

interface PhpLine {
  n: number;
  type: "method" | "cond" | "stmt";
  count: number;
  tc: number;
  fc: number;
}

/** The statement lines of a PHP class of the demo with canned hits. */
function phpLines(p: string): PhpLine[] {
  const lines = statementLines(mockFiles[p], /^(\/\/|\*|\/\*|#)/, /^(<\?php|namespace|use |declare|[{}]|\);?$|\];?$|class |final class|abstract class|enum |interface |trait |\/\*\*)/);
  const untested = UNTESTED.test(p);
  return lines.map((n) => {
    const r = noise(p, n);
    const t = mockFiles[p].split("\n")[n - 1].trim();
    if (/^(public|protected|private)?\s*(static\s+)?function /.test(t)) return { n, type: "method", count: untested || r < 0.15 ? 0 : 1 + Math.floor(r * 9), tc: 0, fc: 0 };
    if (/^(if|elseif|\} elseif|while|foreach|for|match|return .*\?)/.test(t) || t.includes(" ?? ") || t.includes(" ? ")) {
      const tc = untested ? 0 : r < 0.8 ? 1 + Math.floor(r * 4) : 0;
      return { n, type: "cond", count: tc, tc, fc: untested ? 0 : r < 0.45 ? 1 : 0 };
    }
    return { n, type: "stmt", count: untested || r < 0.18 ? 0 : 1 + Math.floor(r * 12), tc: 0, fc: 0 };
  });
}

const phpClasses = () => Object.keys(mockFiles).filter((p) => p.startsWith("app/") && p.endsWith(".php"));

/** Clover XML of the PHP demo's `app/` classes, as PHPUnit `--coverage-clover` writes it. */
export function demoClover(root: string): string {
  const xml = phpClasses()
    .map((p) => {
      const rows = phpLines(p)
        .map((l) =>
          l.type === "method"
            ? `      <line num="${l.n}" type="method" name="m${l.n}" visibility="public" complexity="1" crap="1" count="${l.count}"/>`
            : l.type === "cond"
              ? `      <line num="${l.n}" type="cond" truecount="${l.tc}" falsecount="${l.fc}"/>`
              : `      <line num="${l.n}" type="stmt" count="${l.count}"/>`,
        )
        .join("\n");
      return `    <file name="${root}/${p}">\n${rows}\n    </file>`;
    })
    .join("\n");
  return `<?xml version="1.0" encoding="UTF-8"?>\n<coverage generated="1761472800">\n  <project timestamp="1761472800">\n${xml}\n  </project>\n</coverage>\n`;
}

/** The demo's PHPUnit tests (`Tests\Feature\CartTest::test_total`). */
function phpTests(): string[] {
  const out: string[] = [];
  for (const [p, text] of Object.entries(mockFiles)) {
    if (!/^tests\/.*Test\.php$/.test(p)) continue;
    const ns = /namespace\s+([\w\\]+);/.exec(text)?.[1];
    const cls = /class\s+(\w+)/.exec(text)?.[1];
    if (!cls) continue;
    for (const m of text.matchAll(/function\s+(test\w*)\s*\(/g)) out.push(`${ns ? `${ns}\\` : ""}${cls}::${m[1]}`);
  }
  return out.length ? out : ["Tests\\Unit\\ExampleTest::test_that_true_is_true"];
}

/**
 * PHPUnit's XML coverage report (`--coverage-xml`) of the same run as {@link demoClover}: the
 * tests of every covered line, as file name → text (`index.xml`, `Models/User.php.xml`).
 */
export function demoPhpunitXml(root: string): Record<string, string> {
  const tests = phpTests();
  const files: Record<string, string> = {};
  const entries: string[] = [];
  for (const p of phpClasses()) {
    const rel = p.slice("app/".length);
    const covered = phpLines(p).filter((l) => l.count > 0);
    if (!covered.length) continue;
    const rows = covered
      .map((l) => {
        const k = 1 + Math.floor(noise(p, l.n + 7) * Math.min(3, tests.length));
        const start = Math.floor(noise(rel, 1) * tests.length);
        const by = Array.from({ length: k }, (_, i) => tests[(start + i) % tests.length]);
        return `      <line nr="${l.n}">\n${[...new Set(by)].map((t) => `        <covered by="${t}"/>`).join("\n")}\n      </line>`;
      })
      .join("\n");
    files[`${rel}.xml`] = `<?xml version="1.0"?>\n<phpunit xmlns="https://schema.phpunit.de/coverage/1.0">\n  <file name="${rel.split("/").pop()}" path="/${rel.split("/").slice(0, -1).join("/")}">\n    <coverage>\n${rows}\n    </coverage>\n  </file>\n</phpunit>\n`;
    entries.push(`      <file name="${rel.split("/").pop()}" href="${rel}.xml"/>`);
  }
  files["index.xml"] = `<?xml version="1.0"?>\n<phpunit xmlns="https://schema.phpunit.de/coverage/1.0">\n  <project source="${root}/app">\n    <tests>\n${tests.map((t) => `      <test name="${t}" size="unknown" status="success" time="0.01"/>`).join("\n")}\n    </tests>\n    <directory name="/">\n${entries.join("\n")}\n    </directory>\n  </project>\n</phpunit>\n`;
  return files;
}

/** Source files of the demo for a test framework's coverage (no tests, no declarations-only files). */
function sourcesFor(exts: RegExp): string[] {
  return Object.keys(mockFiles).filter(
    (p) =>
      exts.test(p) &&
      !/(^|\/)(node_modules|dist|target|build|vendor|e2e|tests?|__tests__)\//.test(p) &&
      !/(\.|_)(test|spec)\.\w+$|_test\.go$|\.d\.ts$|(^|\/)(types|index)\.ts$|config\.\w+$/.test(p),
  );
}

const COMMENT = /^(\/\/|#|\*|\/\*|--)/;
const NOT_STATEMENT = /^(import |package |use |mod |export \{|export type|export interface|interface |type |[{}()[\];,]+$|\}.*\{$|end\)?,?$|\]|<\/?\w+|@)/;
const FUNCTION = /^(export\s+)?(default\s+)?(async\s+)?(pub(\(crate\))?\s+)?(function\*?|func|fn|def)\s+(\(\w+ \*?\w+\)\s+)?(\w+)/;
const BRANCH = /^(\} else )?(if|else if|elif|switch|match|while|for)\b|\?\?| \? /;
/** Files of a project that no test of the demo reaches. */
const UNTESTED_SOURCE = /hud|world|server|App\.tsx|main\.\w+$|report/i;

interface SourceLine {
  n: number;
  hits: number;
  fn?: string;
  /** Branches taken of 2. */
  branch?: number;
}

function sourceLines(p: string): SourceLine[] {
  const untested = UNTESTED_SOURCE.test(p);
  return statementLines(mockFiles[p], COMMENT, NOT_STATEMENT).map((n) => {
    const t = mockFiles[p].split("\n")[n - 1].trim();
    const r = noise(p, n);
    const hits = untested || r < 0.16 ? 0 : 1 + Math.floor(r * 9);
    const fn = FUNCTION.exec(t)?.[8];
    const branch = BRANCH.test(t) ? (hits === 0 ? 0 : r < 0.55 ? 1 : 2) : undefined;
    return { n, hits, fn, branch };
  });
}

/** LCOV with functions and branches of the project's sources (c8, nyc, cargo-llvm-cov, …); paths relative to the project. */
export function demoLcovOf(exts: RegExp): string {
  return sourcesFor(exts)
    .map((p) => {
      const lines = sourceLines(p);
      const fns = lines.filter((l) => l.fn);
      const out = ["TN:", `SF:${p}`];
      for (const l of fns) out.push(`FN:${l.n},${l.fn}`);
      for (const l of fns) out.push(`FNDA:${l.hits},${l.fn}`);
      for (const l of lines) out.push(`DA:${l.n},${l.hits}`);
      for (const l of lines.filter((x) => x.branch !== undefined)) {
        out.push(`BRDA:${l.n},0,0,${l.hits && l.branch! > 0 ? l.hits : "-"}`, `BRDA:${l.n},0,1,${l.branch === 2 ? 1 : l.hits ? 0 : "-"}`);
      }
      out.push("end_of_record");
      return out.join("\n");
    })
    .join("\n");
}

/** Istanbul's `coverage-final.json` of the project's TypeScript / JavaScript sources (Jest, Vitest). */
export function demoIstanbul(root: string): string {
  const report: Record<string, unknown> = {};
  for (const p of sourcesFor(/\.(ts|tsx|js|jsx|mjs|cjs|vue)$/)) {
    const path = `${root}/${p}`;
    const lines = sourceLines(p);
    const text = mockFiles[p].split("\n");
    const loc = (n: number) => ({ start: { line: n, column: text[n - 1].length - text[n - 1].trimStart().length }, end: { line: n, column: text[n - 1].length } });
    const statementMap: Record<string, unknown> = {};
    const s: Record<string, number> = {};
    const fnMap: Record<string, unknown> = {};
    const f: Record<string, number> = {};
    const branchMap: Record<string, unknown> = {};
    const b: Record<string, number[]> = {};
    lines.forEach((l, i) => {
      statementMap[i] = loc(l.n);
      s[i] = l.hits;
    });
    lines
      .filter((l) => l.fn)
      .forEach((l, i) => {
        fnMap[i] = { name: l.fn, decl: loc(l.n), loc: loc(l.n), line: l.n };
        f[i] = l.hits;
      });
    lines
      .filter((l) => l.branch !== undefined)
      .forEach((l, i) => {
        branchMap[i] = { loc: loc(l.n), type: "if", locations: [loc(l.n), loc(l.n)], line: l.n };
        b[i] = [l.branch! > 0 ? l.hits : 0, l.branch === 2 ? 1 : 0];
      });
    report[path] = { path, statementMap, fnMap, branchMap, s, f, b };
  }
  return JSON.stringify(report);
}

/** The canned report of a Run with Coverage run of a manifest framework or a Node.js configuration. */
export function demoReportFor(cfg: RunConfiguration, root: string): string {
  switch (cfg.type) {
    case "busted":
      return demoLcov();
    case "vitest":
    case "jest":
      return demoIstanbul(root);
    case "go-test":
      return demoLcovOf(/\.go$/);
    case "cargo-test":
      return demoLcovOf(/\.rs$/);
    default:
      return demoLcovOf(/\.(ts|tsx|js|jsx|mjs|cjs)$/);
  }
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

type Parsed = Omit<CoverageSuite, "id" | "name" | "at" | "files"> & { report: { files: unknown[] } };

function addSuite(name: string, parsed: Parsed, project: string): CoverageSuite {
  const { report, ...fields } = parsed;
  const info: CoverageSuite = { ...fields, id: `coverage-${++seq}`, name, at: Date.now(), files: report.files.length };
  suites.unshift({ info, report, project });
  suites.splice(10);
  return info;
}

/** Add a suite from a report text (paths resolved against `root` / `cwd` and `mappings`). */
export async function addMockCoverage(
  name: string,
  text: string,
  opts: {
    root: string;
    project: string;
    cwd?: string;
    mappings?: { remote: string; local: string }[];
    rules?: { pattern: string; replacement: string }[];
    /** PHPUnit's XML report (file name → text): the tests of each line. */
    testsXml?: Record<string, string>;
  },
): Promise<CoverageSuite> {
  const wasm = await forgeWasm();
  const parsed = callWasmSync<Parsed>(wasm.coverage_parse, {
    text,
    root: opts.root,
    cwd: opts.cwd ?? null,
    mappings: opts.mappings ?? [],
    rules: opts.rules ?? [],
    testsXml: opts.testsXml ?? {},
  });
  return addSuite(name, parsed, opts.project);
}

const find = (id: unknown) => (id ? suites.find((s) => s.info.id === id) : suites[0]);

registerMock("coverage_suites", () => suites.map((s) => s.info));
registerMock("coverage_view", async ({ id }): Promise<CoverageView | null> => {
  const s = find(id);
  if (!s) return null;
  const wasm = await forgeWasm();
  return {
    suite: s.info,
    nodes: callWasmSync(wasm.coverage_summary, { report: s.report, project: s.project }),
    hints: callWasmSync(wasm.coverage_hints, { report: s.report }),
  };
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
registerMock("coverage_merge", async ({ ids }) => {
  const chosen = (ids as string[]).map((id) => suites.find((s) => s.info.id === id)).filter((s): s is Suite => !!s);
  if (chosen.length < 2) throw new Error("choose at least two coverage suites");
  const parsed = callWasmSync<Parsed>((await forgeWasm()).coverage_merge, { reports: chosen.map((s) => s.report) });
  return addSuite(chosen.map((s) => s.info.name).join(" + "), parsed, chosen[0].project);
});
registerMock("coverage_export_html", async ({ id }) => {
  const s = find(id);
  if (!s) throw new Error("no coverage suite");
  const at = new Date(s.info.at);
  const generated = `${at.toISOString().slice(0, 16).replace("T", " ")} UTC`;
  return callWasmSync<string>((await forgeWasm()).coverage_html, { report: s.report, project: s.project, title: s.info.name, generated, sources: mockFiles });
});
registerMock("coverage_import", async ({ path }) => {
  const text = mockFiles[String(path)];
  if (text === undefined) throw new Error(`${String(path)}: no such file`);
  const { projectRoot } = await import("./runMock");
  const root = projectRoot();
  return addMockCoverage(String(path).split("/").pop() ?? String(path), text, { root, project: root.split("/").pop() ?? "" });
});
