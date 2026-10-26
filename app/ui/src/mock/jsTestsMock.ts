/**
 * Canned runs of the Node.js test runners in browser mode (the `?project=node` demo):
 * Vitest writes its JSON report, Playwright its JSON report; both are read into the test
 * tree by Rust (`forge-output`, WebAssembly) like in the desktop app. The tests a
 * configuration selects (scope, test name, Rerun Failed Tests pattern) are honoured.
 */
import type { RunConfiguration } from "@/ipc/run";
import type { Step } from "./runMock";

interface JsTest {
  file: string;
  /** Describe blocks, outermost first. */
  suites: string[];
  title: string;
  line: number;
  ms: number;
  fail?: { message: string; expected: string; actual: string; at: number };
  skipped?: boolean;
}

const fullName = (t: JsTest) => [...t.suites, t.title].join(" ");

const VITEST: JsTest[] = [
  { file: "src/orders/pricing.test.ts", suites: ["pricing"], title: "multiplies the unit price", line: 8, ms: 2 },
  { file: "src/orders/pricing.test.ts", suites: ["pricing", "orderTotal"], title: "adds the tax", line: 13, ms: 1 },
  {
    file: "src/orders/pricing.test.ts",
    suites: ["pricing", "orderTotal"],
    title: "applies the voucher before the tax",
    line: 17,
    ms: 4,
    fail: { message: "AssertionError: expected 64.23 to be 64.24 // Object.is equality", expected: "64.24", actual: "64.23", at: 19 },
  },
  { file: "src/orders/pricing.test.ts", suites: ["pricing", "orderTotal"], title: "rounds to cents", line: 22, ms: 1 },
  { file: "src/orders/store.test.ts", suites: ["store"], title: "finds an order by id", line: 5, ms: 3 },
  { file: "src/orders/store.test.ts", suites: ["store"], title: "returns undefined for unknown ids", line: 9, ms: 1 },
];

const PLAYWRIGHT: JsTest[] = [
  { file: "e2e/checkout.spec.ts", suites: [], title: "reports the health", line: 3, ms: 412 },
  {
    file: "e2e/checkout.spec.ts",
    suites: ["order totals"],
    title: "returns the total of an order",
    line: 9,
    ms: 538,
    fail: { message: "Error: expect(received).toEqual(expected) // deep equality", expected: '{"id": 1042, "total": 64.24}', actual: '{"id": 1042, "total": 64.23}', at: 11 },
  },
  { file: "e2e/checkout.spec.ts", suites: ["order totals"], title: "answers 404 for unknown orders", line: 14, ms: 97 },
];

/** The tests a configuration selects. */
function selected(cfg: RunConfiguration, all: JsTest[]): JsTest[] {
  const o = cfg.options ?? {};
  let tests = all;
  const files = (o.files ?? "").split("\n").filter(Boolean);
  if (files.length) tests = tests.filter((t) => files.includes(t.file));
  switch (o.scope || "all") {
    case "file":
      tests = tests.filter((t) => t.file === o.file);
      break;
    case "directory":
      tests = tests.filter((t) => t.file.startsWith(`${(o.directory ?? "").replace(/\/$/, "")}/`));
      break;
    case "test":
      tests = tests.filter((t) => t.file === o.file && fullName(t).startsWith(o.test ?? ""));
      break;
    case "pattern": {
      let re: RegExp | null = null;
      try {
        re = new RegExp(o.pattern ?? "");
      } catch {
        re = null;
      }
      tests = tests.filter((t) => (re ? re.test(fullName(t)) || re.test(t.title) : true));
      break;
    }
  }
  return tests;
}

const red = (s: string) => `\x1b[31m${s}\x1b[39m`;
const green = (s: string) => `\x1b[32m${s}\x1b[39m`;
const dim = (s: string) => `\x1b[2m${s}\x1b[22m`;

/** Vitest's JSON report (Jest's format). */
function vitestReport(root: string, tests: JsTest[]): string {
  const files = [...new Set(tests.map((t) => t.file))];
  return JSON.stringify({
    numTotalTests: tests.length,
    numFailedTests: tests.filter((t) => t.fail).length,
    success: !tests.some((t) => t.fail),
    testResults: files.map((file) => ({
      name: `${root}/${file}`,
      status: tests.some((t) => t.file === file && t.fail) ? "failed" : "passed",
      message: "",
      assertionResults: tests
        .filter((t) => t.file === file)
        .map((t) => ({
          ancestorTitles: t.suites,
          fullName: fullName(t),
          title: t.title,
          status: t.fail ? "failed" : "passed",
          duration: t.ms,
          location: { line: t.line, column: 5 },
          failureMessages: t.fail ? [`${t.fail.message}\n\n- Expected\n+ Received\n\nExpected: ${t.fail.expected}\nReceived: ${t.fail.actual}\n\n    at ${root}/${file}:${t.fail.at}:28`] : [],
        })),
    })),
  });
}

function vitestSteps(root: string, cfg: RunConfiguration): Step[] {
  const tests = selected(cfg, VITEST);
  const failed = tests.filter((t) => t.fail);
  const steps: Step[] = [[150, `\n ${"\x1b[46m\x1b[30m RUN \x1b[39m\x1b[49m"} ${"\x1b[36mv2.1.8\x1b[39m"} ${dim(root)}\n\n`]];
  for (const file of [...new Set(tests.map((t) => t.file))]) {
    const own = tests.filter((t) => t.file === file);
    const bad = own.filter((t) => t.fail).length;
    steps.push([120, ` ${bad ? red("❯") : green("✓")} ${file} ${dim(`(${own.length} tests${bad ? ` | ${red(`${bad} failed`)}` : ""})`)} ${dim(`${own.reduce((n, t) => n + t.ms, 0)}ms`)}\n`]);
    for (const t of own.filter((x) => x.fail)) steps.push([10, `   ${red("×")} ${fullName(t)} ${dim(`${t.ms}ms`)}\n     ${red(`→ expected ${t.fail!.actual} to be ${t.fail!.expected}`)}\n`]);
  }
  steps.push(
    [40, `\n ${dim("Test Files")}  ${failed.length ? `${red(`${new Set(failed.map((t) => t.file)).size} failed`)} | ` : ""}${green(`${new Set(tests.filter((t) => !t.fail).map((t) => t.file)).size} passed`)}\n`],
    [10, ` ${dim("     Tests")}  ${failed.length ? `${red(`${failed.length} failed`)} | ` : ""}${green(`${tests.length - failed.length} passed`)} (${tests.length})\n`],
    [10, { report: vitestReport(root, tests) }],
    [10, { code: failed.length ? 1 : 0 }],
  );
  return steps;
}

/** Playwright's JSON report. */
function playwrightReport(root: string, tests: JsTest[]): string {
  const spec = (t: JsTest) => ({
    title: t.title,
    ok: !t.fail,
    tags: [],
    id: `${t.file}:${t.line}`,
    file: t.file.replace(/^e2e\//, ""),
    line: t.line,
    column: 5,
    tests: [
      {
        timeout: 30000,
        annotations: [],
        expectedStatus: "passed",
        projectId: "chromium",
        projectName: "chromium",
        status: t.fail ? "unexpected" : "expected",
        results: [
          {
            workerIndex: 0,
            status: t.fail ? "failed" : "passed",
            duration: t.ms,
            retry: 0,
            ...(t.fail
              ? {
                  error: {
                    message: `${t.fail.message}\n\n- Expected  - 1\n+ Received  + 1\n\nExpected: ${t.fail.expected}\nReceived: ${t.fail.actual}`,
                    stack: `${t.fail.message}\n    at ${root}/${t.file}:${t.fail.at}:36`,
                    location: { file: `${root}/${t.file}`, line: t.fail.at, column: 36 },
                  },
                }
              : {}),
          },
        ],
      },
    ],
  });
  const files = [...new Set(tests.map((t) => t.file))];
  return JSON.stringify({
    config: { rootDir: `${root}/e2e` },
    suites: files.map((file) => {
      const own = tests.filter((t) => t.file === file);
      const describes = [...new Set(own.filter((t) => t.suites.length).map((t) => t.suites[0]))];
      return {
        title: file.replace(/^e2e\//, ""),
        file: file.replace(/^e2e\//, ""),
        line: 0,
        column: 0,
        specs: own.filter((t) => !t.suites.length).map(spec),
        suites: describes.map((d) => ({
          title: d,
          file: file.replace(/^e2e\//, ""),
          line: Math.min(...own.filter((t) => t.suites[0] === d).map((t) => t.line)) - 1,
          column: 6,
          specs: own.filter((t) => t.suites[0] === d).map(spec),
        })),
      };
    }),
    errors: [],
    stats: { expected: tests.filter((t) => !t.fail).length, unexpected: tests.filter((t) => t.fail).length, skipped: 0, flaky: 0 },
  });
}

function playwrightSteps(root: string, cfg: RunConfiguration): Step[] {
  const tests = selected(cfg, PLAYWRIGHT);
  const failed = tests.filter((t) => t.fail);
  const steps: Step[] = [[400, `\nRunning ${tests.length} tests using 1 worker\n\n`]];
  tests.forEach((t, i) => {
    const title = [t.file.replace(/^e2e\//, ""), ...t.suites, t.title].join(" › ");
    steps.push([t.ms, `  ${t.fail ? red("✘") : green("✓")}  ${i + 1} [chromium] › ${title} ${dim(`(${t.ms}ms)`)}\n`]);
  });
  if (failed.length) steps.push([20, `\n  ${red(`${failed.length} failed`)}\n`]);
  steps.push(
    [10, `  ${green(`${tests.length - failed.length} passed`)} (2.1s)\n\nTo open last HTML report run:\n\n  npx playwright show-report\n\n`],
    [10, { report: playwrightReport(root, tests) }],
    [10, { code: failed.length ? 1 : 0 }],
  );
  return steps;
}

/** Canned output of a Node.js test runner's run, or `null` for other types. */
export function jsTestSteps(cfg: RunConfiguration, root: string): Step[] | null {
  switch (cfg.type) {
    case "vitest":
    case "jest":
      return vitestSteps(root, cfg);
    case "playwright":
      return playwrightSteps(root, cfg);
    default:
      return null;
  }
}
