import { describe, expect, it } from "vitest";
import { AnsiParser, appendSegments, color256, lineText, type ConsoleLine } from "./ansi";
import { findFileLinks, toWorkspacePath } from "./fileLinks";
import { findTests, testAt, testConfiguration } from "./phpTests";
import {
  countTests,
  formatDuration,
  parseLocationHint,
  parseServiceMessage,
  TestTreeBuilder,
  unescapeValue,
} from "./teamcity";

describe("ansi", () => {
  it("parses colors and resets", () => {
    const p = new AnsiParser();
    const segs = p.feed("plain \x1b[1;31mred bold\x1b[0m done");
    expect(segs.map((s) => s.text)).toEqual(["plain ", "red bold", " done"]);
    expect(segs[1].style).toEqual({ bold: true, fg: "var(--ansi-red)" });
    expect(segs[2].style).toEqual({});
  });

  it("keeps escape sequences split across chunks", () => {
    const p = new AnsiParser();
    expect(p.feed("a\x1b[3").map((s) => s.text)).toEqual(["a"]);
    const segs = p.feed("2mgreen");
    expect(segs[0]).toEqual({ text: "green", style: { fg: "var(--ansi-green)" } });
  });

  it("supports 256 colors, truecolor and drops OSC and cursor sequences", () => {
    const p = new AnsiParser();
    const segs = p.feed("\x1b]0;title\x07\x1b[2K\x1b[38;5;196mx\x1b[48;2;1;2;3my");
    expect(segs[0].style.fg).toBe("rgb(255, 0, 0)");
    expect(segs[1].style.bg).toBe("rgb(1, 2, 3)");
    expect(segs.map((s) => s.text).join("")).toBe("xy");
    expect(color256(9)).toBe("var(--ansi-bright-red)");
    expect(color256(232)).toBe("rgb(8, 8, 8)");
  });

  it("splits lines and handles carriage returns", () => {
    const lines: ConsoleLine[] = [];
    const p = new AnsiParser();
    appendSegments(lines, p.feed("first\nprogress 10%\rprogress 50%"));
    appendSegments(lines, p.feed("\rprogress 100%\r"));
    appendSegments(lines, p.feed("\nlast\r\nend"));
    expect(lines.map(lineText)).toEqual(["first", "progress 100%", "last", "end"]);
  });
});

describe("file links", () => {
  it("finds paths with lines in common formats", () => {
    const text = "Error in /home/u/shop/app/Models/User.php on line 42 and tests/Unit/UserTest.php:21";
    const links = findFileLinks(text);
    expect(links.map((l) => [l.path, l.line])).toEqual([
      ["/home/u/shop/app/Models/User.php", 42],
      ["tests/Unit/UserTest.php", 21],
    ]);
    expect(text.slice(links[1].start, links[1].end)).toBe("tests/Unit/UserTest.php:21");
    expect(findFileLinks("#1 /p/vendor/x/Foo.php(31): bar()")[0]).toMatchObject({ path: "/p/vendor/x/Foo.php", line: 31 });
    expect(findFileLinks("src/main.ts:3:14")[0]).toMatchObject({ line: 3, column: 14 });
  });

  it("ignores URLs, versions and bare names", () => {
    expect(findFileLinks("see https://example.com/docs/a.html")).toEqual([]);
    expect(findFileLinks("PHP 8.3.12 (cli) version 2.8.4")).toEqual([]);
    expect(findFileLinks("README.md")).toEqual([]);
    expect(findFileLinks("modified:   app/Models/User.php")[0]).toMatchObject({ path: "app/Models/User.php", line: undefined });
  });

  it("maps paths into the workspace", () => {
    expect(toWorkspacePath("/home/u/shop/app/User.php", "/home/u/shop")).toBe("app/User.php");
    expect(toWorkspacePath("C:\\work\\shop\\app\\User.php", "C:\\work\\shop")).toBe("app/User.php");
    expect(toWorkspacePath("./tests/A.php", "/r")).toBe("tests/A.php");
    expect(toWorkspacePath("../x.php", "/r", "sub")).toBe("x.php");
    expect(toWorkspacePath("/etc/passwd.txt", "/r")).toBeNull();
    expect(toWorkspacePath("../../x.php", "/r")).toBeNull();
  });
});

describe("teamcity", () => {
  it("parses service messages with escapes", () => {
    const m = parseServiceMessage(
      "##teamcity[testFailed name='test_a' message='Failed asserting that |'a|' is |'b|'.' details=' /p/A.php:21|n' flowId='7']",
    );
    expect(m).toEqual({
      name: "testFailed",
      attrs: { name: "test_a", message: "Failed asserting that 'a' is 'b'.", details: " /p/A.php:21\n", flowId: "7" },
    });
    expect(parseServiceMessage("plain output")).toBeNull();
    expect(unescapeValue("|[x|]|||0x00e9")).toBe("[x]|é");
  });

  it("parses location hints", () => {
    expect(parseLocationHint("php_qn:///p/tests/UserTest.php::\\Tests\\UserTest::test_a")).toEqual({
      path: "/p/tests/UserTest.php",
      member: "test_a",
    });
    expect(parseLocationHint("file:///p/a.test.ts:12")).toEqual({ path: "/p/a.test.ts", line: 12 });
    expect(parseLocationHint("php_qn:///p/T.php::\\T::test_x with data set #1")?.member).toBe("test_x");
  });

  it("builds a tree with statuses and durations", () => {
    const b = new TestTreeBuilder();
    const lines = [
      "##teamcity[testCount count='3']",
      "##teamcity[testSuiteStarted name='Tests\\Unit\\UserTest' locationHint='php_qn:///p/UserTest.php::\\Tests\\Unit\\UserTest']",
      "##teamcity[testStarted name='test_ok' locationHint='php_qn:///p/UserTest.php::\\Tests\\Unit\\UserTest::test_ok']",
      "##teamcity[testFinished name='test_ok' duration='12']",
      "##teamcity[testStarted name='test_bad']",
      "##teamcity[testFailed name='test_bad' message='nope' type='comparisonFailure' expected='a' actual='b']",
      "##teamcity[testFinished name='test_bad' duration='3']",
      "##teamcity[testIgnored name='test_skip' message='later']",
      "##teamcity[testSuiteFinished name='Tests\\Unit\\UserTest']",
    ];
    for (const l of lines) b.apply(parseServiceMessage(l)!);
    b.finish(1);
    const suite = b.root.children[0];
    expect(b.expectedCount).toBe(3);
    expect(suite.name).toBe("Tests\\Unit\\UserTest");
    expect(suite.status).toBe("failed");
    expect(suite.duration).toBe(15);
    expect(suite.children.map((c) => [c.name, c.status])).toEqual([
      ["test_ok", "passed"],
      ["test_bad", "failed"],
      ["test_skip", "ignored"],
    ]);
    expect(suite.children[1]).toMatchObject({ expected: "a", actual: "b", message: "nope" });
    expect(countTests(b.root)).toMatchObject({ total: 3, passed: 1, failed: 1, ignored: 1 });
    expect(b.root.status).toBe("failed");
  });

  it("formats durations like PHPStorm", () => {
    expect(formatDuration(87)).toBe("87 ms");
    expect(formatDuration(1245)).toBe("1 s 245 ms");
    expect(formatDuration(2000)).toBe("2 s");
    expect(formatDuration(undefined)).toBe("");
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
