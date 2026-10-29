import { describe, expect, it } from "vitest";
import { invoke, listen } from "@/ipc";
import { runEvent, type CommandLine, type RunConfiguration, type RunEvent, type TestSummary } from "@/ipc/run";
import type { FrameworkConfig, StepDefinition, StoredRun, TestFile, TestRunEntry } from "@/ipc/testFrameworks";
import { mockFiles } from "./workspaceMock";
import "./runMock";
import "./testFrameworksMock";

async function run(runId: string, configuration: RunConfiguration, ms = 1500): Promise<{ events: RunEvent[]; commandLine: string }> {
  const events: RunEvent[] = [];
  const off = await listen<RunEvent>(runEvent(runId), (e) => events.push(e));
  const started = await invoke<{ commandLine: string }>("run_start", { request: { runId, configuration, testRunner: true } });
  await new Promise((r) => setTimeout(r, ms));
  off();
  return { events, commandLine: started.commandLine };
}

const summaryOf = (events: RunEvent[]): TestSummary | undefined =>
  events.flatMap((e) => (e.type === "tests" ? [e.summary] : [])).at(-1);

describe("PHP test frameworks mock (Rust through WebAssembly)", () => {
  it("detects the demo project's frameworks", async () => {
    const list = await invoke<FrameworkConfig[]>("test_frameworks_list");
    expect(list.map((c) => [c.framework, c.configFile, c.detected])).toEqual([
      ["phpunit", "phpunit.xml", true],
      ["pest", "phpunit.xml", true],
      ["codeception", "codeception.yml", true],
      ["behat", "behat.yml", true],
      ["phpspec", "phpspec.yml", true],
    ]);
  });

  it("discovers Pest tests and Behat scenarios for the gutter", async () => {
    const pest = await invoke<TestFile>("test_discover", { path: "tests/Feature/CartTest.php", text: mockFiles["tests/Feature/CartTest.php"] });
    expect(pest.framework).toBe("pest");
    expect(pest.targets.map((t) => [t.kind, t.label])).toEqual([
      ["test", "an empty cart costs nothing"],
      ["describe", "totals"],
      ["test", "it adds the tax"],
      ["dataset", "`totals` → it adds the tax with ten euros"],
      ["dataset", "`totals` → it adds the tax with a hundred euros"],
      ["test", "it rounds to cents"],
      ["test", "it belongs to a user"],
    ]);
    const behat = await invoke<TestFile>("test_discover", { path: "features/checkout.feature", text: mockFiles["features/checkout.feature"] });
    expect(behat.targets.map((t) => [t.kind, t.line])).toEqual([
      ["feature", 1],
      ["scenario", 9],
      ["scenario", 14],
      ["example", 20],
      ["example", 21],
    ]);
  });

  it("finds the step definitions of a step", async () => {
    const text = mockFiles["features/checkout.feature"];
    const line = text.split("\n").findIndex((l) => l.includes("Then I should have 1 product"));
    const defs = await invoke<StepDefinition[]>("behat_step_definitions", { text, line });
    expect(defs.map((d) => [d.path, d.method, d.line])).toEqual([["features/bootstrap/FeatureContext.php", "iShouldHaveProductsInTheBasket", 37]]);
  });

  it("builds command lines in Rust", async () => {
    const cmd = await invoke<CommandLine>("run_command_line", {
      request: { configuration: { name: "b", type: "behat", options: { scope: "scenario", file: "features/checkout.feature", line: "10" } } },
    });
    expect(cmd.args).toEqual([
      "vendor/bin/behat",
      "--config",
      "behat.yml",
      "--format=progress",
      "--out=std",
      "--format=junit",
      "--out=/tmp/forge-test-reports/run",
      "features/checkout.feature:10",
    ]);
  });

  it("runs Pest, Behat (JUnit file) and PHPSpec (JUnit on stdout) into test trees and the history", async () => {
    const pest = await run("tf-pest", { name: "CartTest", type: "pest", options: { scope: "file", file: "tests/Feature/CartTest.php" } });
    expect(pest.commandLine).toContain("vendor/bin/pest --teamcity");
    expect(summaryOf(pest.events)?.text).toMatch(/^Tests failed: 1, passed: 4 of 5 tests/);

    const behat = await run("tf-behat", { name: "checkout", type: "behat", options: { scope: "file", file: "features/checkout.feature" } });
    expect(summaryOf(behat.events)?.text).toMatch(/^Tests failed: 1, passed: 2 of 3 tests/);

    const spec = await run("tf-spec", { name: "CheckoutSpec", type: "phpspec", options: { scope: "file", file: "spec/App/Services/CheckoutSpec.php" } });
    const nodes = spec.events.flatMap((e) => (e.type === "tests" ? e.nodes : []));
    expect(nodes.find((n) => n.name === "it rejects negative amounts")).toMatchObject({
      status: "failed",
      location: { workspacePath: "spec/App/Services/CheckoutSpec.php", member: "it_rejects_negative_amounts" },
    });
    const console = spec.events.flatMap((e) => (e.type === "console" ? e.lines.flatMap((l) => l.segments.map((s) => s.text)) : [])).join("\n");
    expect(console).not.toContain("<testsuite");

    const history = await invoke<TestRunEntry[]>("test_history_list");
    expect(history.map((h) => h.name)).toEqual(["CheckoutSpec", "checkout", "CartTest"]);
    const stored = await invoke<StoredRun>("test_history_open", { id: history[1].id });
    expect(stored.configuration.type).toBe("behat");
    expect(stored.summary.text).toBe(summaryOf(behat.events)?.text);
    expect(stored.nodes.find((n) => n.name === "Buying a single product")?.status).toBe("failed");
  });

  it("reruns the failed tests per framework", async () => {
    const cfg = await invoke<RunConfiguration | null>("test_rerun_failed", {
      configuration: { name: "checkout", type: "behat", options: { scope: "file", file: "features/checkout.feature" } },
      failed: [{ name: "Buying a single product" }],
    });
    expect(cfg?.options?.testRunnerOptions).toBe("--rerun");
  });
});
