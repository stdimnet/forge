import { describe, expect, it } from "vitest";
import { invoke } from "@/ipc";
import type { LibraryNode, LibraryRoot, PhpCompletion, PhpProblem, SourceRootEntry } from "@/ipc/phpMeta";
import { setSetting } from "./settingsMock";
import { mockFiles } from "./workspaceMock";
import "./phpMetaMock";

const CONTROLLER = "app/Http/Controllers/PaymentController.php";

describe("PHP metadata mock", () => {
  it("reports deprecated calls, expected values, pure calls and the namespace check", async () => {
    const problems = await invoke<PhpProblem[]>("php_inspect", { path: CONTROLLER });
    const text = mockFiles[CONTROLLER];
    const found = problems.map((p) => [p.code, text.slice(p.start, p.end)]);
    expect(found).toEqual([
      ["PhpDeprecation", "payNow"],
      ["PhpExpectedValuesShouldBeUsed", "'YEN'"],
      ["PhpExpectedValuesShouldBeUsed", "'later'"],
      ["PhpExpressionResultUnused", "$gateway->fee(1999)"],
    ]);
    expect(problems[0].fix?.title).toBe("Replace with '$gateway->charge(1999, 'EUR')'");
    const money = await invoke<PhpProblem[]>("php_inspect", { path: "app/Support/Money.php" });
    expect(money[0]).toMatchObject({ code: "PhpIllegalPsrClassPath", fix: { title: "Set namespace to 'App\\Support'" } });
    expect(await invoke("php_inspect", { path: "app/Models/User.php" })).toEqual([]);
  });

  it("completes expected arguments, return values, map keys and array shape keys", async () => {
    const at = (text: string) => ({ path: CONTROLLER, offset: text.indexOf("|"), content: text.replace("|", "") });
    const labels = async (text: string) => (await invoke<PhpCompletion | null>("php_complete", at(text)))?.items.map((i) => i.label);
    expect(await labels("<?php $gateway->charge(1, '|');")).toEqual(["EUR", "USD", "GBP"]);
    expect(await labels("<?php $gateway->capture('|');")).toEqual(["auto", "manual"]);
    expect(await labels("<?php if ($gateway->status() === '|') {}")).toEqual(["pending", "captured", "failed"]);
    expect(await labels("<?php app('|');")).toEqual(["payments", "invoices"]);
    expect(await labels("<?php $payment['|'];")).toEqual(["id", "cents", "currency"]);
    expect(await labels("<?php $gateway->charge(|);")).toBeUndefined();
  });

  it("lists include paths and source roots", async () => {
    setSetting("php.includePaths", ["/usr/share/php", "/opt/missing"], "project");
    const roots = await invoke<LibraryRoot[]>("php_libraries");
    expect(roots).toEqual([
      { name: "php", path: "/usr/share/php", exists: true },
      { name: "missing", path: "/opt/missing", exists: false },
    ]);
    const entries = await invoke<LibraryNode[]>("php_library_list", { path: "/usr/share/php" });
    expect(entries.map((e) => [e.name, e.dir])).toEqual([
      ["Console", true],
      ["Net", true],
      ["PEAR.php", false],
    ]);
    expect(entries[2].open).toBe("library://file//usr/share/php/PEAR.php");
    await expect(invoke("php_library_list", { path: "/etc" })).rejects.toThrow();
    setSetting("php.includePaths", null, "project");

    const listed = await invoke<SourceRootEntry[]>("php_source_roots", {
      overrides: [{ path: "app/Support", prefix: "App\\Helpers", kind: "source" }],
    });
    expect(listed.map((r) => [r.path, r.prefix, r.kind, r.origin])).toEqual([
      ["app", "App", "source", "composer"],
      ["app/Support", "App\\Helpers", "source", "user"],
      ["tests", "Tests", "test", "composer"],
    ]);
  });
});
