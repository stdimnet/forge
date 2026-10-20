import { describe, expect, it } from "vitest";
import { navigate, type VisibleRow } from "./treeNavigation";

const rows: VisibleRow[] = [
  { path: "app", isDir: true, expanded: true, depth: 1 },
  { path: "app/Models", isDir: true, expanded: false, depth: 2 },
  { path: "app/User.php", isDir: false, expanded: false, depth: 2 },
  { path: "routes", isDir: true, expanded: false, depth: 1 },
  { path: "composer.json", isDir: false, expanded: false, depth: 1 },
];

describe("project tree keyboard navigation", () => {
  it("selects the first row when nothing is selected", () => {
    expect(navigate(rows, null, "ArrowDown")).toEqual({ type: "select", path: "app" });
  });

  it("moves up and down and stops at the ends", () => {
    expect(navigate(rows, "app", "ArrowDown")).toEqual({ type: "select", path: "app/Models" });
    expect(navigate(rows, "app/Models", "ArrowUp")).toEqual({ type: "select", path: "app" });
    expect(navigate(rows, "app", "ArrowUp")).toEqual({ type: "none" });
    expect(navigate(rows, "composer.json", "ArrowDown")).toEqual({ type: "none" });
  });

  it("expands closed folders and enters open ones", () => {
    expect(navigate(rows, "app/Models", "ArrowRight")).toEqual({ type: "expand", path: "app/Models" });
    expect(navigate(rows, "app", "ArrowRight")).toEqual({ type: "select", path: "app/Models" });
  });

  it("collapses open folders and jumps to the parent otherwise", () => {
    expect(navigate(rows, "app", "ArrowLeft")).toEqual({ type: "collapse", path: "app" });
    expect(navigate(rows, "app/User.php", "ArrowLeft")).toEqual({ type: "select", path: "app" });
  });

  it("opens files and toggles folders with Enter", () => {
    expect(navigate(rows, "composer.json", "Enter")).toEqual({ type: "open", path: "composer.json" });
    expect(navigate(rows, "routes", "Enter")).toEqual({ type: "expand", path: "routes" });
    expect(navigate(rows, "app", "Enter")).toEqual({ type: "collapse", path: "app" });
  });
});
