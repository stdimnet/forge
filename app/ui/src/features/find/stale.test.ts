import { describe, expect, it } from "vitest";
import { dispatchFsChanges } from "@/state/fsWatch";
import { findResults, resultsStale, setFindResults, setResultsStale } from "./window";

const changes = (changed: string[], removed: string[] = []) => ({ changed, removed, git: false });

describe("stale results", () => {
  it("marks a shown result stale when project files change, and ignores changes without a result", () => {
    setResultsStale(false);
    dispatchFsChanges(changes(["a.php"]));
    expect(resultsStale()).toBe(false);

    setFindResults({ ...findResults(), searched: true, busy: false });
    dispatchFsChanges({ changed: [], removed: [], git: true });
    expect(resultsStale()).toBe(false);
    dispatchFsChanges(changes(["a.php"]));
    expect(resultsStale()).toBe(true);
  });

  it("also counts a deleted file", () => {
    setResultsStale(false);
    setFindResults({ ...findResults(), searched: true, busy: false });
    dispatchFsChanges(changes([], ["gone.php"]));
    expect(resultsStale()).toBe(true);
  });
});
