import { describe, expect, it } from "vitest";
import { registerSearchProvider, runSearch, type SearchItem } from "./providers";

const item = (id: string, score: number, kind: SearchItem["kind"] = "class"): SearchItem => ({
  id,
  label: id,
  score,
  kind,
  run: () => {},
});

describe("search providers", () => {
  registerSearchProvider({ id: "t.classes", tab: "classes", title: "Classes", search: () => [item("A", 1), item("B", 5)] });
  registerSearchProvider({ id: "t.classes2", tab: "classes", title: "Classes", search: async () => [item("C", 3)] });
  registerSearchProvider({ id: "t.symbols", tab: "symbols", title: "Symbols", search: () => [item("B", 9), item("m", 2, "method")] });
  registerSearchProvider({
    id: "t.broken",
    tab: "text",
    title: "Text",
    search: () => {
      throw new Error("boom");
    },
  });

  it("merges providers of one tab and sorts by score", async () => {
    const groups = await runSearch("classes", "x", { includeNonProject: false, perGroup: 10 });
    expect(groups.map((g) => g.items.map((i) => i.id))).toEqual([["B", "C", "A"]]);
  });

  it("orders groups by tab, dedupes and truncates in All", async () => {
    const groups = await runSearch("all", "x", { includeNonProject: false, perGroup: 2 });
    const mine = groups.filter((g) => g.provider.id.startsWith("t."));
    expect(mine.map((g) => g.provider.tab)).toEqual(["classes", "symbols"]);
    expect(mine[0].items.map((i) => i.id)).toEqual(["B", "C"]);
    expect(mine[0].more).toBe(true);
    // "B" was already listed as a class.
    expect(mine[1].items.map((i) => i.id)).toEqual(["m"]);
  });

  it("replaces providers registered with the same id", async () => {
    registerSearchProvider({ id: "t.classes2", tab: "classes", title: "Classes", search: () => [] });
    const groups = await runSearch("classes", "x", { includeNonProject: false, perGroup: 10 });
    expect(groups[0].items.map((i) => i.id)).toEqual(["B", "A"]);
  });
});
