import { describe, expect, it } from "vitest";
import { affectedDirs, dispatchFsChanges, onFsChanged, parentDir } from "./fsWatch";

describe("file watcher events", () => {
  it("derives the directories to re-list", () => {
    expect(parentDir("app/Models/User.php")).toBe("app/Models");
    expect(parentDir("composer.json")).toBe("");
    expect(
      affectedDirs({ changed: ["app/Models/User.php", "app/Models/Order.php", "README.md"], removed: ["app/Old"], git: false }).sort(),
    ).toEqual(["", "app", "app/Models"]);
  });

  it("delivers batches to subscribers until they unsubscribe", () => {
    const seen: string[][] = [];
    const off = onFsChanged((c) => seen.push(c.changed));
    dispatchFsChanges({ changed: ["a.php"], removed: [], git: false });
    off();
    dispatchFsChanges({ changed: ["b.php"], removed: [], git: false });
    expect(seen).toEqual([["a.php"]]);
  });
});
