import { describe, expect, it } from "vitest";
import { invoke } from "@/ipc";
import type { NewTestPlan, TestSource, TestTargets } from "@/ipc/codeintel";
import { mockFiles } from "./workspaceMock";
import "./goToTestMock";

describe("go to test mock (forge_codeintel::test_nav through WebAssembly)", () => {
  it("finds the demo project's tests and subjects", async () => {
    const order = await invoke<TestTargets>("code_test_targets", { path: "app/Models/Order.php" });
    expect(order).toMatchObject({ isTest: false, name: "Order", canCreate: true });
    expect(order.targets.map((t) => t.path)).toEqual(["tests/Feature/OrderTest.php"]);
    const back = await invoke<TestTargets>("code_test_targets", { path: "tests/Feature/OrderTest.php" });
    expect(back.isTest).toBe(true);
    expect(back.targets.map((t) => t.path)).toEqual(["app/Models/Order.php"]);
  });

  it("plans a Laravel unit test and creates it in memory only", async () => {
    const plan = await invoke<NewTestPlan>("code_new_test_plan", { path: "app/Services/InvoiceService.php" });
    expect(plan).toMatchObject({
      framework: "PHPUnit",
      className: "InvoiceServiceTest",
      namespace: "Tests\\Unit\\Services",
      directory: "tests/Unit/Services",
      subjectFqn: "App\\Services\\InvoiceService",
    });
    const request = { ...plan, members: plan.members.slice(0, 1), setUp: true, tearDown: false };
    const file = await invoke<TestSource>("code_create_test", { request });
    expect(file.path).toBe("tests/Unit/Services/InvoiceServiceTest.php");
    expect(mockFiles[file.path]).toContain("class InvoiceServiceTest extends");
    await expect(invoke("code_create_test", { request })).rejects.toThrow("already exists");
    delete mockFiles[file.path];
  });
});
