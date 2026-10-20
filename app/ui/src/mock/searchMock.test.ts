import { describe, expect, it } from "vitest";
import { invoke } from "@/ipc";
import type { FileMatch } from "@/ipc/search";
import { indexDemoSymbols } from "./searchMock";

describe("fs_find_files mock", () => {
  it("finds demo files by camel humps", async () => {
    const hits = await invoke<FileMatch[]>("fs_find_files", { query: "OrCo", limit: 5 });
    expect(hits[0].path).toBe("app/Http/Controllers/OrderController.php");
    expect(await invoke<FileMatch[]>("fs_find_files", { query: "", limit: 5 })).toEqual([]);
  });
});

describe("demo symbol index", () => {
  it("finds classes, enums, methods, properties and constants", () => {
    const syms = indexDemoSymbols({
      "app/X.php": `<?php
namespace App;
final class OrderController
{
    private string $currency;
    const LIMIT = 3;
    public function show(): void {}
}
enum Status: string
{
    case Paid = 'paid';
}
function helper() {}
`,
    });
    const by = (n: string) => syms.find((s) => s.name === n);
    expect(by("OrderController")).toMatchObject({ kind: "class", container: "App", line: 3 });
    expect(by("$currency")).toMatchObject({ kind: "property", container: "OrderController" });
    expect(by("LIMIT")).toMatchObject({ kind: "constant" });
    expect(by("show")).toMatchObject({ kind: "method", container: "OrderController" });
    expect(by("Status")).toMatchObject({ kind: "enum" });
    expect(by("Paid")).toMatchObject({ kind: "constant", container: "Status" });
  });
});
