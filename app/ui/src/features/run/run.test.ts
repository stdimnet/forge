import { describe, expect, it } from "vitest";
import type { ConsoleLine, RunEvent } from "@/ipc/run";
import { forgeWasm } from "@/wasm";
import type { TestFile } from "@/ipc/testFrameworks";
import { applyConsoleDelta, applyTestUpdates, emptyTestTree, lineText, testAt } from "./tests";
import { resultKeys } from "./testResults";

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

  const discover = async (path: string, text: string): Promise<TestFile> => {
    const wasm = await forgeWasm();
    return JSON.parse(wasm.test_discover(JSON.stringify({ path, text })));
  };

  it("finds classes and test methods (Rust, via WebAssembly)", async () => {
    const tests = (await discover("tests/Unit/UserTest.php", src)).targets;
    expect(tests.map((t) => [t.kind, t.label, t.line])).toEqual([
      ["class", "UserTest", 3],
      ["method", "test_a", 5],
      ["method", "b", 7],
      ["method", "c", 10],
    ]);
    expect((await discover("tests/Helper.php", "<?php\nclass Helper {\n public function testX() {}\n}")).targets).toEqual([]);
  });

  it("picks the test at the caret with its configuration", async () => {
    const tests = (await discover("tests/Unit/UserTest.php", src)).targets;
    expect(testAt(tests, 4)?.kind).toBe("class");
    expect(testAt(tests, 8)?.label).toBe("b");
    expect(testAt(tests, 5)!.configuration).toEqual({
      name: "UserTest::test_a",
      type: "phpunit",
      options: { scope: "method", class: "Tests\\Unit\\UserTest", method: "test_a", file: "tests/Unit/UserTest.php" },
    });
  });

  it("names results like the discovered tests", () => {
    const tree = emptyTestTree();
    applyTestUpdates(
      tree,
      [
        { id: 1, parentId: 0, name: "Tests\\Feature\\CartTest", kind: "suite", status: "failed", durationText: "", console: [], location: { path: "tests/Feature/CartTest.php", workspacePath: "tests/Feature/CartTest.php" } },
        { id: 2, parentId: 1, name: 'it adds with data set "one"', kind: "test", status: "passed", durationText: "", console: [], location: { path: "x", member: "it adds" } },
        { id: 3, parentId: 1, name: 'it adds with data set "two"', kind: "test", status: "failed", durationText: "", console: [], location: { path: "x", member: "it adds" } },
      ],
      tree.summary,
    );
    const keys = resultKeys(tree.root);
    expect(keys.get("tests/Feature/CartTest.php")).toBe("failed");
    expect(keys.get("tests/Feature/CartTest.php::it adds")).toBe("failed");
  });
});
