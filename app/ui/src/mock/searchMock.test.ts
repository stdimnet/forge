import { describe, expect, it } from "vitest";
import { invoke } from "@/ipc";
import type { FileMatch, SymbolHit } from "@/ipc/search";
import "./searchMock";
import "./findMock";

describe("fs_find_files mock", () => {
  it("finds demo files by camel humps", async () => {
    const hits = await invoke<FileMatch[]>("fs_find_files", { query: "OrCo", limit: 5 });
    expect(hits[0].path).toBe("app/Http/Controllers/OrderController.php");
    expect(await invoke<FileMatch[]>("fs_find_files", { query: "", limit: 5 })).toEqual([]);
  });
});

describe("search_find_symbols mock", () => {
  it("finds demo classes through the Rust index", async () => {
    const hits = await invoke<SymbolHit[]>("search_find_symbols", { query: "OrderCon", typesOnly: true, limit: 5, skipExtensions: [] });
    expect(hits[0]).toMatchObject({ name: "OrderController", kind: "class", path: "app/Http/Controllers/OrderController.php" });
    expect(hits[0].line).toBeGreaterThan(0);
    expect(await invoke<SymbolHit[]>("search_find_symbols", { query: "OrderCon", typesOnly: true, limit: 5, skipExtensions: ["php"] })).toEqual([]);
  });
});
