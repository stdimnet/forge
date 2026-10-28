/**
 * The Ruby demo's context and canned runs, without side effects (imported by the test
 * framework mock, the run mock and rubyMock.ts): RSpec (progress dots, then the JSON
 * report the Rust parser reads), Minitest (verbose output), Ruby scripts, Rake tasks and
 * Bundler commands.
 */
import type { RunConfiguration } from "@/ipc/run";
import type { Step } from "./runMock";
import { mockSettingValue } from "./settingsMock";

export const RUBY_ROOT = "/home/anna/Projects/pantry";
const RBENV = "/home/anna/.rbenv/versions/3.3.6";

export const RUBY_TOOLS: Record<string, string> = {
  gem: `${RBENV}/bin/gem`,
  bundle: `${RBENV}/bin/bundle`,
  rake: `${RBENV}/bin/rake`,
  rspec: `${RBENV}/bin/rspec`,
  rubocop: `${RBENV}/bin/rubocop`,
  rdbg: `${RBENV}/bin/rdbg`,
  irb: `${RBENV}/bin/irb`,
  erb: `${RBENV}/bin/erb`,
};

/** The project's Ruby context as `commands::ruby::context` builds it. */
export function rubyContext() {
  const chosen = String(mockSettingValue("ruby.interpreter") ?? "") || `${RBENV}/bin/ruby`;
  return {
    ruby: chosen,
    bundle: String(mockSettingValue("ruby.bundler.path") ?? ""),
    useBundler: mockSettingValue("ruby.bundler.useBundleExec") !== false,
    tools: RUBY_TOOLS,
    env: { PATH: `${RBENV}/bin:/usr/local/bin:/usr/bin:/bin` },
    gems: ["rake", "rspec", "rspec-core", "minitest", "rubocop", "debug", "money"],
    windows: false,
  };
}

// ---- RSpec ------------------------------------------------------------------------------------

interface Example {
  group: string;
  description: string;
  line: number;
  status: "passed" | "failed" | "pending";
  ms: number;
}

const SPEC = "./spec/pantry/inventory_spec.rb";
const EXAMPLES: Example[] = [
  { group: "Pantry::Inventory", description: "starts empty", line: 6, status: "passed", ms: 1 },
  { group: "Pantry::Inventory #shopping_list", description: "lists the items below their minimum", line: 11, status: "passed", ms: 2 },
  { group: "Pantry::Inventory #shopping_list", description: "skips items that are stocked", line: 16, status: "passed", ms: 1 },
  { group: "Pantry::Inventory #total_cents", description: "adds up what is missing", line: 23, status: "failed", ms: 4 },
  { group: "Pantry::Inventory #total_cents with a voucher", description: "takes ten percent off", line: 29, status: "pending", ms: 0 },
];

function selectExamples(cfg: RunConfiguration): Example[] {
  const o = cfg.options ?? {};
  const full = (e: Example) => `${e.group} ${e.description}`;
  switch (o.scope || "all") {
    case "test": {
      if (o.line) {
        const line = Number(o.line);
        const hit = EXAMPLES.find((e) => e.line === line);
        if (hit) return [hit];
        // A group's line: its examples.
        const group = (o.test ?? "").trim();
        return EXAMPLES.filter((e) => full(e).startsWith(group));
      }
      return EXAMPLES.filter((e) => full(e).includes(o.test ?? ""));
    }
    case "pattern": {
      const wanted = (o.examples || o.pattern || "").split("\n").filter(Boolean);
      return EXAMPLES.filter((e) => wanted.some((w) => full(e).includes(w)));
    }
    default:
      return EXAMPLES;
  }
}

/** The RSpec JSON report of a run (`--format json --out …`). */
function rspecReport(list: Example[]): string {
  const examples = list.map((e, i) => ({
    id: `${SPEC}[1:${i + 1}]`,
    description: e.description,
    full_description: `${e.group} ${e.description}`,
    status: e.status,
    file_path: SPEC,
    line_number: e.line,
    run_time: e.ms / 1000,
    pending_message: e.status === "pending" ? "Not yet implemented" : null,
    ...(e.status === "failed"
      ? {
          exception: {
            class: "RSpec::Expectations::ExpectationNotMetError",
            message: "\nexpected: 1050\n     got: 525\n\n(compared using ==)\n",
            backtrace: [`${SPEC}:25:in 'block (3 levels) in <top (required)>'`],
          },
        }
      : {}),
  }));
  const failures = list.filter((e) => e.status === "failed").length;
  const pending = list.filter((e) => e.status === "pending").length;
  return JSON.stringify({
    version: "3.13.2",
    seed: 40213,
    examples,
    summary: { duration: 0.0213, example_count: list.length, failure_count: failures, pending_count: pending, errors_outside_of_examples_count: 0 },
    summary_line: `${list.length} examples, ${failures} failure${failures === 1 ? "" : "s"}${pending ? `, ${pending} pending` : ""}`,
  });
}

/** Canned `rspec --format progress --format json` run. */
export function rspecSteps(cfg: RunConfiguration): Step[] {
  const list = selectExamples(cfg);
  const dots = list.map((e) => (e.status === "passed" ? "." : e.status === "failed" ? "\x1b[31mF\x1b[0m" : "\x1b[33m*\x1b[0m")).join("");
  const failed = list.filter((e) => e.status === "failed");
  const steps: Step[] = [
    [250, `Run options: include {:locations=>${JSON.stringify(cfg.options?.line ? { [SPEC]: [Number(cfg.options.line)] } : {})}}\n\n`],
    [400, `${dots}\n\n`],
  ];
  if (failed.length)
    steps.push([
      30,
      `Failures:\n\n  1) Pantry::Inventory #total_cents adds up what is missing\n     Failure/Error: expect(inventory.total_cents).to eq(1050)\n\n       expected: 1050\n            got: 525\n\n       (compared using ==)\n     # ${SPEC}:25:in 'block (3 levels) in <top (required)>'\n\n`,
    ]);
  steps.push(
    [20, `Finished in 0.0213 seconds (files took 0.10414 seconds to load)\n${list.length} examples, ${failed.length} failure${failed.length === 1 ? "" : "s"}\n`],
    [10, { report: rspecReport(list) }],
    [10, { code: failed.length ? 1 : 0 }],
  );
  return steps;
}

// ---- Minitest --------------------------------------------------------------------------------------

interface MiniTest {
  method: string;
  line: number;
  fail?: string;
}

const MINITESTS: MiniTest[] = [
  { method: "test_low", line: 11 },
  { method: "test_missing", line: 15 },
  { method: "test_price", line: 19, fail: 'Expected: "1.29"\n  Actual: "€1.29"' },
];

/** Canned `ruby -Itest … -v` output (Minitest's verbose reporter). */
export function minitestSteps(cfg: RunConfiguration): Step[] {
  const o = cfg.options ?? {};
  let tests = MINITESTS;
  if (o.scope === "test" && o.test) tests = tests.filter((t) => t.method === o.test);
  if (o.scope === "pattern" && o.pattern) {
    const re = new RegExp(o.pattern.replace(/^\/|\/$/g, ""));
    tests = tests.filter((t) => re.test(t.method));
  }
  const steps: Step[] = [[200, "Run options: -v --seed 18329\n\n# Running:\n\n"]];
  for (const t of tests) steps.push([120, `Pantry::ItemTest#${t.method} = 0.00 s = ${t.fail ? "F" : "."}\n`]);
  const failed = tests.filter((t) => t.fail);
  steps.push([20, `\nFinished in 0.004213s, ${Math.round(tests.length / 0.004213)} runs/s, ${tests.length * 237} assertions/s.\n`]);
  failed.forEach((t, i) => steps.push([10, `\n  ${i + 1}) Failure:\nPantry::ItemTest#${t.method} [${RUBY_ROOT}/test/pantry/item_test.rb:${t.line + 1}]:\n${t.fail}\n`]));
  steps.push([10, `\n${tests.length} runs, ${tests.length} assertions, ${failed.length} failures, 0 errors, 0 skips\n`], [10, { code: failed.length ? 1 : 0 }]);
  return steps;
}

// ---- Scripts, Rake, Bundler ------------------------------------------------------------------------

/** A Ruby script or Rake task run. */
export function rubyRunSteps(cfg: RunConfiguration): Step[] {
  const o = cfg.options ?? {};
  const debug = cfg.env?.FORGE_RDBG_PORT
    ? [[60, `DEBUGGER: Debugger can attach via TCP/IP (127.0.0.1:${cfg.env.FORGE_RDBG_PORT})\nDEBUGGER: wait for debugger connection...\nDEBUGGER: Connected.\n`, "stderr"] as Step]
    : [];
  if (cfg.type === "rake") {
    const task = o.task || "default";
    if (task === "shopping_list") return [...debug, [500, "2 x Rice\n1 x Coffee\n"], [10, { code: 0 }]];
    if (task.startsWith("db:")) return [...debug, [600, `${RBENV}/bin/ruby bin/restock --seed\nSeeded 12 items.\n`], [10, { code: 0 }]];
    return [...debug, [800, `Running ${task}…\n`], [10, { code: 0 }]];
  }
  return [...debug, [450, "Shopping list:\n"], [20, "  2 x Rice\n"], [20, "  1 x Coffee\n"], [20, "Total: €9.23\n"], [10, { code: 0 }]];
}

/** Bundler, gem and Rake commands of the hidden `ruby-command` type. */
export function rubyCommandSteps(args: string[]): Step[] {
  const sub = args[0] ?? "";
  if (sub === "install" && args.includes("ruby-lsp"))
    return [
      [900, "Fetching ruby-lsp-0.23.11.gem\nFetching prism-1.4.0.gem\nFetching rbs-3.9.2.gem\n"],
      [1200, "Building native extensions. This could take a while...\nSuccessfully installed prism-1.4.0\nSuccessfully installed rbs-3.9.2\nSuccessfully installed ruby-lsp-0.23.11\n3 gems installed\n"],
      [10, { code: 0 }],
    ];
  if (sub === "install" && args[1] && args[1] !== "--no-document")
    return [[700, `Fetching ${args[1]}\nSuccessfully installed ${args[1]}\n1 gem installed\n`], [10, { code: 0 }]];
  if (sub === "install")
    return [
      [300, "Fetching gem metadata from https://rubygems.org/.........\n"],
      [500, "Resolving dependencies...\n"],
      [300, "Bundle complete! 7 Gemfile dependencies, 31 gems now installed.\nUse `bundle info [gemname]` to see where a bundled gem is installed.\n"],
      [10, { code: 0 }],
    ];
  if (sub === "update")
    return [
      [300, "Fetching gem metadata from https://rubygems.org/.........\nResolving dependencies...\n"],
      [500, "Using rake 13.2.1\nFetching rubocop 1.68.0 (was 1.66.1)\nInstalling rubocop 1.68.0 (was 1.66.1)\nBundle updated!\n"],
      [10, { code: 0 }],
    ];
  if (sub === "outdated")
    return [
      [600, "Fetching gem metadata from https://rubygems.org/.........\nResolving dependencies...\n\n"],
      [20, "Gem       Current  Latest  Requested  Groups\nrubocop   1.66.1   1.68.0  ~> 1.66    development, test\nminitest  5.25.1   5.25.4  ~> 5.25    test\n"],
      [10, { code: 1 }],
    ];
  if (sub === "exec" && args[1] === "rake") return rubyRunSteps({ name: "rake", type: "rake", options: { task: args[2] ?? "default" } });
  return [[400, "Done.\n"], [10, { code: 0 }]];
}
