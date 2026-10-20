import { describe, expect, it } from "vitest";
import type { ConsoleLine, RunEvent } from "@/ipc/run";
import { forgeWasm } from "@/wasm";
import { findTests, testAt, testConfiguration } from "./phpTests";
import { applyConsoleDelta, applyTestUpdates, emptyTestTree, lineText } from "./tests";

describe("run output (Rust, forge-output via WebAssembly)", () => {
  it("renders console lines and a test tree", async () => {
    const wasm = await forgeWasm();
    const handle = Number(wasm.run_output_new(JSON.stringify({ testRunner: true, root: "/p" })));
    const feed = (input: object): RunEvent[] => JSON.parse(wasm.run_output_handle(JSON.stringify({ handle, input })));
    const events = [
      ...feed({ type: "output", stream: "stdout", text: "\x1b[32mok\x1b[0m /p/app/A.php:3\n##teamcity[testStarted name='t']\n" }),
      ...feed({ type: "output", stream: "stdout", text: "##teamcity[testFinished name='t' duration='4']\n" }),
      ...feed({ type: "exited", code: 0, signal: null }),
    ];
    const lines: ConsoleLine[] = [];
    const tree = emptyTestTree();
    for (const e of events) {
      if (e.type === "console") applyConsoleDelta(lines, e);
      if (e.type === "tests") applyTestUpdates(tree, e.nodes, e.summary);
    }
    expect(lines.map(lineText)).toEqual(["ok /p/app/A.php:3", "", "Process finished with exit code 0", ""]);
    expect(lines[0].segments[0].style).toEqual({ fg: "var(--ansi-green)" });
    expect(lines[0].segments.find((s) => s.link)?.link).toEqual({ target: "app/A.php", line: 3 });
    expect(tree.root.children.map((c) => [c.name, c.status, c.durationText])).toEqual([["t", "passed", "4 ms"]]);
    expect(tree.summary.text).toBe("Tests passed: 1 of 1 test – 4 ms");
    wasm.run_output_drop(JSON.stringify({ handle }));
  });
});

describe("php test detection", () => {
  const src = [
    "<?php",
    "namespace Tests\\Unit;",
    "use PHPUnit\\Framework\\TestCase;",
    "final class UserTest extends TestCase",
    "{",
    "    public function test_a(): void {}",
    "    #[Test]",
    "    public function b(): void {}",
    "    public function helper(): void {}",
    "    /** @test */",
    "    public function c(): void {}",
    "}",
  ].join("\n");

  it("finds classes and test methods", () => {
    const tests = findTests(src);
    expect(tests.map((t) => [t.kind, t.method ?? t.fqcn, t.line])).toEqual([
      ["class", "Tests\\Unit\\UserTest", 3],
      ["method", "test_a", 5],
      ["method", "b", 7],
      ["method", "c", 10],
    ]);
    expect(findTests("<?php\nclass Helper {\n public function testX() {}\n}")).toEqual([]);
  });

  it("picks the test at the caret and builds configurations", () => {
    const tests = findTests(src);
    expect(testAt(tests, 4)?.kind).toBe("class");
    expect(testAt(tests, 8)?.method).toBe("b");
    expect(testConfiguration(testAt(tests, 5)!, "tests/Unit/UserTest.php")).toEqual({
      name: "UserTest::test_a",
      type: "phpunit",
      options: { scope: "method", class: "Tests\\Unit\\UserTest", method: "test_a", file: "tests/Unit/UserTest.php" },
    });
  });
});
