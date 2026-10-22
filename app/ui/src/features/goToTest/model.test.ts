import { describe, expect, it } from "vitest";
import type { TestTargets } from "@/ipc/codeintel";
import { chooserTitle, hookNames, memberStub, nothingFound } from "./model";

const found = (over: Partial<TestTargets>): TestTargets => ({ isTest: false, name: "Order", targets: [], canCreate: true, ...over });
const item = { kind: "test" as const, group: "Tests", name: "OrderTest.php", path: "tests/Unit/OrderTest.php", line: 0, column: 0 };

describe("go to test", () => {
  it("titles the chooser like PHPStorm", () => {
    expect(chooserTitle(found({}))).toBe("Choose Test for Order");
    expect(chooserTitle(found({ targets: [item, item] }))).toBe("Choose Test for Order (2 found)");
    expect(chooserTitle(found({ isTest: true, name: "OrderTest", targets: [item, item] }))).toBe(
      "Choose Test Subject for OrderTest (2 found)",
    );
    expect(nothingFound(found({ isTest: true }))).toBe("No test subjects found");
  });

  it("names hooks and stubs per framework", () => {
    expect(hookNames("PHPUnit")).toEqual(["setUp()", "tearDown()"]);
    expect(hookNames("Pest")).toEqual(["beforeEach()", "afterEach()"]);
    expect(memberStub("PHPUnit", "findByNumber")).toBe("testFindByNumber()");
    expect(memberStub("Pest", "findByNumber")).toBe("test('find by number')");
    expect(memberStub("Vitest", "format")).toBe('it("format")');
  });
});
