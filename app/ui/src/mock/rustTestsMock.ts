/**
 * The Rust demo's Cargo context and canned `cargo test` / `cargo run` output, without side
 * effects (imported by the test framework mock, the run mock and rustMock.ts). The output
 * is what cargo prints on a stable toolchain: plain libtest text, with cargo's `Running`
 * lines on stderr.
 */
import type { RunConfiguration } from "@/ipc/run";
import { rustDemoFiles } from "./demoRustProject";
import type { Step } from "./runMock";
import { mockSettingValue } from "./settingsMock";

export const RUST_ROOT = "/home/anna/Projects/shop";

const target = (name: string, kind: string, srcPath: string) => ({ name, kind, srcPath });

/** The members of the demo workspace as `cargo metadata` reports them. */
export const RUST_PACKAGES = [
  {
    name: "shop-core",
    dir: "crates/shop-core",
    targets: [target("shop_core", "lib", "crates/shop-core/src/lib.rs"), target("checkout", "test", "crates/shop-core/tests/checkout.rs")],
  },
  {
    name: "shop-cli",
    dir: "crates/shop-cli",
    targets: [target("shop", "bin", "crates/shop-cli/src/main.rs"), target("report", "example", "crates/shop-cli/examples/report.rs")],
  },
];

const setting = <T>(key: string, fallback: T): T => (mockSettingValue(key) as T | undefined) ?? fallback;

/** The project's Cargo context as `commands::rust::context` builds it. */
export function rustContext() {
  return {
    cargo: "/home/anna/.cargo/bin/cargo",
    toolchain: setting("rust.toolchain", ""),
    nightly: false,
    testFormat: setting("rust.test.format", "auto"),
    testRunner: setting("rust.test.runner", "cargo"),
    nextest: false,
    llvmCov: true,
    env: {},
    features: setting<string[]>("rust.cargo.features", []),
    allFeatures: setting("rust.cargo.allFeatures", false),
    noDefaultFeatures: setting("rust.cargo.noDefaultFeatures", false),
    workspace: "",
    packages: RUST_PACKAGES,
    files: Object.keys(rustDemoFiles).filter((f) => f.endsWith(".rs")),
    windows: false,
  };
}

// ---- cargo test ---------------------------------------------------------------------------------

interface RustCase {
  /** `lib`, `test:checkout` or `doc`. */
  target: string;
  name: string;
  result: "ok" | "FAILED" | "ignored";
  message?: string;
}

const CASES: RustCase[] = [
  { target: "lib", name: "cart::tests::vouchers", result: "ignored", message: "needs the voucher service" },
  { target: "lib", name: "cart::tests::rounds_to_cents", result: "FAILED" },
  { target: "lib", name: "money::tests::formats_negative_amounts", result: "ok" },
  { target: "lib", name: "money::tests::discounts_round_half_up", result: "ok" },
  { target: "lib", name: "cart::tests::adds_items", result: "ok" },
  { target: "test:checkout", name: "checkout_works", result: "ok" },
  { target: "doc", name: "crates/shop-core/src/lib.rs - price (line 18)", result: "ok" },
  { target: "doc", name: "crates/shop-core/src/cart.rs - cart::Cart::total (line 31)", result: "ok" },
  { target: "doc", name: "crates/shop-core/src/lib.rs - (line 3)", result: "ok" },
];

const FAILURE = `thread 'cart::tests::rounds_to_cents' (3081) panicked at crates/shop-core/src/cart.rs:69:9:
assertion \`left == right\` failed
  left: 588
 right: 589
note: run with \`RUST_BACKTRACE=1\` environment variable to display a backtrace
`;

function selectCases(cfg: RunConfiguration): RustCase[] {
  const o = cfg.options ?? {};
  const scope = o.scope || "all";
  if ((o.package || "") === "shop-cli" && scope !== "all") return [];
  let list = CASES;
  if (scope === "target" || o.target) list = list.filter((c) => !o.target || c.target === o.target || (o.target === "lib" && c.target === "lib"));
  const names = (o.tests ?? "").split("\n").filter(Boolean);
  if (scope === "pattern" && names.length) return list.filter((c) => names.includes(c.name));
  if (scope === "test" || scope === "pattern") {
    const t = o.test ?? "";
    list = list.filter((c) => (o.exact === "true" ? c.name === t : c.name.includes(t)));
  }
  return list;
}

const SUITES: [string, string][] = [
  ["lib", "     Running unittests src/lib.rs (target/debug/deps/shop_core-49a7891832f2bb85)\n"],
  ["test:checkout", "     Running tests/checkout.rs (target/debug/deps/checkout-aff14352b157d497)\n"],
  ["doc", "   Doc-tests shop_core\n"],
];

/** Canned `cargo test` output for a Cargo Test configuration. */
export function cargoTestSteps(cfg: RunConfiguration): Step[] {
  const cases = selectCases(cfg);
  const steps: Step[] = [
    [200, "   Compiling shop-core v0.3.0 (/home/anna/Projects/shop/crates/shop-core)\n", "stderr"],
    [700, "    Finished `test` profile [unoptimized + debuginfo] target(s) in 1.84s\n", "stderr"],
  ];
  let failed = false;
  for (const [suite, running] of SUITES) {
    const list = cases.filter((c) => c.target === suite);
    if (!list.length) continue;
    steps.push([60, running, "stderr"], [20, `\nrunning ${list.length} test${list.length === 1 ? "" : "s"}\n`]);
    for (const c of list) {
      const result = c.result === "ignored" ? `ignored, ${c.message}` : c.result;
      steps.push([suite === "doc" ? 160 : 40, `test ${c.name} ... ${result}\n`]);
    }
    const bad = list.filter((c) => c.result === "FAILED");
    if (bad.length) {
      failed = true;
      steps.push([10, "\nfailures:\n\n"]);
      for (const c of bad) steps.push([10, `---- ${c.name} stdout ----\n\n${FAILURE}\n`]);
      steps.push([10, `\nfailures:\n${bad.map((c) => `    ${c.name}\n`).join("")}\n`]);
    }
    const count = (r: RustCase["result"]) => list.filter((c) => c.result === r).length;
    steps.push([
      10,
      `test result: ${bad.length ? "FAILED" : "ok"}. ${count("ok")} passed; ${count("FAILED")} failed; ${count("ignored")} ignored; 0 measured; 0 filtered out; finished in 0.0${suite === "doc" ? "9" : "0"}s\n\n`,
    ]);
    if (bad.length) steps.push([10, "error: test failed, to rerun pass `-p shop-core --lib`\n", "stderr"]);
  }
  if (!cases.length) steps.push([20, "\nrunning 0 tests\n\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n\n"]);
  steps.push([10, { code: failed ? 101 : 0 }]);
  return steps;
}

/** Canned `cargo run` output of the demo binary and example. */
export function cargoRunSteps(cfg: RunConfiguration): Step[] {
  const o = cfg.options ?? {};
  const command = (o.command || "run").split(/\s+/)[0];
  const compile: Step[] = [
    [300, "   Compiling shop-core v0.3.0 (/home/anna/Projects/shop/crates/shop-core)\n", "stderr"],
    [500, "   Compiling shop-cli v0.3.0 (/home/anna/Projects/shop/crates/shop-cli)\n", "stderr"],
    [600, `    Finished \`${o.profile === "release" ? "release" : "dev"}\` profile [${o.profile === "release" ? "optimized" : "unoptimized + debuginfo"}] target(s) in 2.31s\n`, "stderr"],
  ];
  if (command !== "run") return [...compile, [10, { code: 0 }]];
  if ((o.target ?? "") === "example:report")
    return [...compile, [40, "     Running `target/debug/examples/report`\n", "stderr"], [60, "Coffee 9.00 EUR\nCroissant 6.60 EUR\n"], [10, { code: 0 }]];
  return [...compile, [40, "     Running `target/debug/shop`\n", "stderr"], [80, "Anna Schmidt: 2 items, 15.60 EUR\n"], [10, { code: 0 }]];
}
