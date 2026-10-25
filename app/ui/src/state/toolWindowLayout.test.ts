import { describe, expect, it } from "vitest";
import {
  anchorOf,
  clampFloating,
  defaultFloatingBounds,
  dropIndex,
  hidesOnFocusLoss,
  isDockedMode,
  modeOf,
  setFloatingBounds,
  setViewMode,
  groupIds,
  dropBefore,
  isSplitSlot,
  moveOpenWindow,
  moveToolWindow,
  placementOf,
  setRemoved,
  slotFor,
  type ActiveWindows,
  type ToolWindowDefault,
} from "./toolWindowLayout";

const windows: ToolWindowDefault[] = [
  { id: "project", anchor: "left", order: 0 },
  { id: "commit", anchor: "left", order: 10 },
  { id: "structure", anchor: "left", order: 40 },
  { id: "terminal", anchor: "bottom", order: 30 },
  { id: "problems", anchor: "bottom", order: 50 },
  { id: "git", anchor: "bottom", order: 60 },
  { id: "ai-chat", anchor: "right", order: 0 },
  { id: "database", anchor: "right", order: 10 },
  { id: "notes", anchor: "right", split: true, order: 0 },
];

const none: ActiveWindows = { left: null, leftSplit: null, right: null, rightSplit: null, bottom: null, bottomSplit: null };

describe("tool window placement", () => {
  it("maps anchors and split groups to slots", () => {
    expect(slotFor("left")).toBe("left");
    expect(slotFor("bottom", true)).toBe("bottomSplit");
    expect(anchorOf("rightSplit")).toBe("right");
    expect(isSplitSlot("leftSplit")).toBe(true);
    expect(isSplitSlot("bottom")).toBe(false);
  });

  it("uses the registration as default", () => {
    expect(placementOf(windows[3], {})).toEqual({ slot: "bottom", order: 30, removed: false });
    expect(groupIds(windows, {}, "left")).toEqual(["project", "commit", "structure"]);
    expect(groupIds(windows, {}, "rightSplit")).toEqual(["notes"]);
    expect(groupIds(windows, {}, "leftSplit")).toEqual([]);
  });

  it("moves a window to another stripe and keeps the rest in order", () => {
    const placed = moveToolWindow(windows, {}, "terminal", "right");
    expect(groupIds(windows, placed, "bottom")).toEqual(["problems", "git"]);
    expect(groupIds(windows, placed, "right")).toEqual(["ai-chat", "database", "terminal"]);
    const first = moveToolWindow(windows, placed, "git", "right", "ai-chat");
    expect(groupIds(windows, first, "right")).toEqual(["git", "ai-chat", "database", "terminal"]);
  });

  it("reorders inside a group", () => {
    // Drag Project below Structure: the drop index counts the dragged window.
    const group = groupIds(windows, {}, "left");
    expect(dropBefore(group, "project", 3)).toBeUndefined();
    const placed = moveToolWindow(windows, {}, "project", "left", dropBefore(group, "project", 3));
    expect(groupIds(windows, placed, "left")).toEqual(["commit", "structure", "project"]);
    const back = moveToolWindow(windows, placed, "project", "left", dropBefore(groupIds(windows, placed, "left"), "project", 0));
    expect(groupIds(windows, back, "left")).toEqual(["project", "commit", "structure"]);
    // Dropped right in front of itself: stays where it is.
    expect(dropBefore(group, "commit", 1)).toBe("structure");
    const same = moveToolWindow(windows, {}, "commit", "left", dropBefore(group, "commit", 1));
    expect(groupIds(windows, same, "left")).toEqual(group);
  });

  it("moves into the split groups", () => {
    const placed = moveToolWindow(windows, {}, "structure", "leftSplit", "no-such-window");
    expect(groupIds(windows, placed, "leftSplit")).toEqual(["structure"]);
    expect(groupIds(windows, placed, "left")).toEqual(["project", "commit"]);
    expect(moveToolWindow(windows, {}, "unknown", "left")).toEqual({});
  });

  it("removes windows from the sidebar and puts them back when moved", () => {
    const removed = setRemoved(windows, {}, "problems", true);
    expect(groupIds(windows, removed, "bottom")).toEqual(["terminal", "git"]);
    expect(setRemoved(windows, removed, "problems", true)).toBe(removed);
    expect(groupIds(windows, setRemoved(windows, removed, "problems", false), "bottom")).toEqual(["terminal", "problems", "git"]);
    const moved = moveToolWindow(windows, removed, "problems", "bottomSplit");
    expect(moved.problems).toEqual({ slot: "bottomSplit", order: 10, removed: false });
  });

  it("keeps a moved window open at its new place", () => {
    const active = { ...none, bottom: "terminal", right: "ai-chat" };
    expect(moveOpenWindow(active, "terminal", "right")).toEqual({ ...none, right: "terminal" });
    expect(moveOpenWindow(active, "git", "right")).toBe(active);
    expect(moveOpenWindow(active, "terminal", "bottom")).toBe(active);
  });

  it("finds the drop index from the button positions", () => {
    const buttons = [
      { start: 0, end: 30 },
      { start: 34, end: 64 },
    ];
    expect(dropIndex(buttons, 5)).toBe(0);
    expect(dropIndex(buttons, 20)).toBe(1);
    expect(dropIndex(buttons, 60)).toBe(2);
    expect(dropIndex([], 10)).toBe(0);
  });
});

describe("view modes", () => {
  it("are Dock Pinned by default and keep the place when changed", () => {
    expect(modeOf(windows[0], {})).toBe("dockPinned");
    const moved = moveToolWindow(windows, {}, "terminal", "right");
    const floating = setViewMode(windows, moved, "terminal", "float");
    expect(floating.terminal).toMatchObject({ slot: "right", mode: "float" });
    expect(modeOf(windows[3], floating)).toBe("float");
    // Back to Dock Pinned drops the mode instead of storing the default.
    const docked = setViewMode(windows, floating, "terminal", "dockPinned");
    expect(docked.terminal.mode).toBeUndefined();
    expect(docked.terminal.slot).toBe("right");
    // Moving a window keeps its mode.
    const again = moveToolWindow(windows, floating, "terminal", "bottom");
    expect(again.terminal.mode).toBe("float");
    expect(setViewMode(windows, {}, "unknown", "float")).toEqual({});
  });

  it("know which modes are docked and which hide on focus loss", () => {
    expect(["dockPinned", "dockUnpinned", "undock", "float", "window"].map((m) => isDockedMode(m as never))).toEqual([true, true, false, false, false]);
    expect(["dockPinned", "dockUnpinned", "undock", "float", "window"].map((m) => hidesOnFocusLoss(m as never))).toEqual([false, true, true, false, false]);
  });

  it("place floating windows on their side and keep them reachable", () => {
    const area = { width: 1400, height: 860 };
    const right = defaultFloatingBounds("right", area);
    expect(right.x + right.width).toBeLessThan(area.width);
    expect(defaultFloatingBounds("left", area).x).toBe(60);
    const bottom = defaultFloatingBounds("bottom", area);
    expect(bottom.y + bottom.height).toBeLessThan(area.height);
    expect(clampFloating({ x: -5000, y: -20, width: 9000, height: 50 }, area)).toEqual({ x: 48 - 1400, y: 0, width: 1400, height: 80 });
    expect(clampFloating({ x: 5000, y: 5000, width: 300, height: 200 }, area)).toEqual({ x: 1352, y: 812, width: 300, height: 200 });
    const stored = setFloatingBounds(windows, {}, "project", { x: 10.4, y: 20.6, width: 300, height: 200 });
    expect(stored.project).toMatchObject({ slot: "left", floating: { x: 10, y: 21, width: 300, height: 200 } });
  });
});
