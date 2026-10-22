import { describe, expect, it } from "vitest";
import { indexTooltip } from "./indexTooltip";

const base = { enabled: true, indexing: false, files: 1200, contentFiles: 1100, binaryFiles: 70, oversizedFiles: 30, lastRefreshMs: 10_000 };

describe("indexTooltip", () => {
  it("lists what is searched, what is by name only and the age of the refresh", () => {
    expect(indexTooltip(base, 15_000)).toBe(
      "Search index: 1,200 files\n1,100 searched by content\n70 binary, by name only\n30 over the size limit, by name only\nUpdated 5 s ago",
    );
  });

  it("says it is updating and leaves out empty lines", () => {
    expect(indexTooltip({ ...base, indexing: true, binaryFiles: 0, oversizedFiles: 0 })).toBe(
      "Search index: 1,200 files\n1,100 searched by content\nUpdating…",
    );
  });
});
