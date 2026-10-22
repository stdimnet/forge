import { describe, expect, it } from "vitest";
import { invoke } from "@/ipc";
import type { FileMatches, FindPaged } from "@/ipc/find";
import type { FileMatch, SymbolHit } from "@/ipc/search";
import "./searchMock";
import "./findMock";

describe("fs_find_files mock", () => {
  it("finds demo files by camel humps", async () => {
    const hits = await invoke<FileMatch[]>("fs_find_files", { query: "OrCo", limit: 5 });
    expect(hits[0].path).toBe("app/Http/Controllers/OrderController.php");
    expect(await invoke<FileMatch[]>("fs_find_files", { query: "", limit: 5 })).toEqual([]);
    expect(await invoke<FileMatch[]>("fs_find_files", { query: "OrCo", limit: 5, exact: true })).toEqual([]);
    expect((await invoke<FileMatch[]>("fs_find_files", { query: "OrderCon", limit: 5, exact: true }))[0].name).toBe("OrderController.php");
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

describe("paged find mock", () => {
  it("serves the rest of a result in pages", async () => {
    const first = await invoke<FindPaged>("find_in_files_paged", { request: { query: "order", limit: 100000 }, pageSize: 10 });
    expect(first.rowCount).toBeGreaterThan(10);
    expect(first.files.reduce((n, f) => n + f.matches.length, 0)).toBe(10);
    const rest = await invoke<FileMatches[]>("find_page", { id: first.id, offset: 10, count: 100000 });
    expect(rest.reduce((n, f) => n + f.matches.length, 0)).toBe(first.rowCount - 10);
    await invoke("find_in_files_paged", { request: { query: "order" }, pageSize: 1 });
    await expect(invoke("find_page", { id: first.id, offset: 0, count: 1 })).rejects.toThrow();
  });
});
