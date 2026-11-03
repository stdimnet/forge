import { describe, expect, it } from "vitest";
import { closeAllTabs, openViewTab } from "@/state/editor";
import { closedTabs, recentFiles } from "./history";

describe("navigation history", () => {
  it("forgets the files of the previous project when another project takes the window", async () => {
    openViewTab({ path: "db-console://a/1", name: "a", view: "x" });
    openViewTab({ path: ".env", name: ".env", view: "x" });
    await Promise.resolve();
    expect(recentFiles()).toContain(".env");
    closeAllTabs();
    await Promise.resolve();
    expect(recentFiles()).toEqual([]);
    expect(closedTabs()).toEqual([]);
  });
});
