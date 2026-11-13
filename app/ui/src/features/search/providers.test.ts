import { describe, expect, it } from "vitest";
import { registerSearchProvider, runSearch, runSearchStreaming, type SearchItem } from "./providers";
import "@/mock/fuzzyMock";

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

describe("streaming search", () => {
  const pause = (ms: number) => new Promise((r) => setTimeout(r, ms));
  registerSearchProvider({ id: "s.fast", tab: "files", title: "Files", order: 0, search: async () => [item("quick", 4, "file")] });
  registerSearchProvider({
    id: "s.slow",
    tab: "files",
    title: "Files",
    order: 1,
    search: async () => {
      await pause(60);
      return [item("late", 9, "file")];
    },
  });

  it("reports the fast provider first and everything on the last update", async () => {
    const updates: { ids: string[]; done: boolean }[] = [];
    await runSearchStreaming("files", "x", { includeNonProject: false, perGroup: 10 }, (groups, done) => {
      updates.push({ ids: groups.flatMap((g) => g.items.map((i) => i.id)).filter((id) => id === "quick" || id === "late"), done });
    });
    expect(updates).toEqual([
      { ids: ["quick"], done: false },
      { ids: ["late", "quick"], done: true },
    ]);
  });

  it("finishes at once when no provider answers the tab", async () => {
    const seen: boolean[] = [];
    await runSearchStreaming("text", "x", { includeNonProject: false, perGroup: 10 }, (_groups, done) => seen.push(done));
    expect(seen.length).toBeGreaterThan(0);
    expect(seen.at(-1)).toBe(true);
  });
});
