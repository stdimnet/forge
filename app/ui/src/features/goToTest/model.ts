/**
 * Go to Test / Create New Test in the UI: the chooser's wording and the dialog's state. The
 * targets, the dialog's initial values and the test source come from Rust
 * (`forge_codeintel::test_nav`).
 */
import { createSignal } from "solid-js";
import type { NewTestPlan, TestFramework, TestTargets } from "@/ipc/codeintel";

/** PHPStorm's chooser title: `Choose Test for Order (2 found)`, `Choose Test Subject for OrderTest`. */
export function chooserTitle(found: TestTargets): string {
  const base = found.isTest ? `Choose Test Subject for ${found.name}` : `Choose Test for ${found.name}`;
  return found.targets.length ? `${base} (${found.targets.length} found)` : base;
}

/** The hint when nothing is found and nothing can be created. */
export const nothingFound = (found: TestTargets): string => (found.isTest ? "No test subjects found" : "No tests found");

export const CREATE_NEW_TEST = "Create New Test...";

export const isPhpFramework = (f: TestFramework): boolean => f === "PHPUnit" || f === "Pest";

/** "Create New PHP Test" for PHP subjects, "Create New Test" for JS/TS. */
export const dialogTitle = (plan: NewTestPlan): string => (isPhpFramework(plan.framework) ? "Create New PHP Test" : "Create New Test");

/** setUp()/tearDown() for PHPUnit, beforeEach()/afterEach() for Pest, Vitest and Jest. */
export const hookNames = (f: TestFramework): [string, string] => (f === "PHPUnit" ? ["setUp()", "tearDown()"] : ["beforeEach()", "afterEach()"]);

/** The test stub a member gets: `testShip()`, `test('ship')`, `it("ship")`. */
export function memberStub(f: TestFramework, name: string): string {
  if (f === "PHPUnit") return `test${name.charAt(0).toUpperCase()}${name.slice(1)}()`;
  if (f === "Pest") return `test('${name.replace(/_/g, " ").replace(/(?!^)([A-Z])/g, " $1").toLowerCase().replace(/\s+/g, " ").trim()}')`;
  return `it("${name}")`;
}

const [createTestPlan, setCreateTestPlan] = createSignal<NewTestPlan | null>(null);
export { createTestPlan, setCreateTestPlan };
