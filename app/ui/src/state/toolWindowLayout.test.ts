import { describe, expect, it } from "vitest";
import {
  anchorOf,
  dropIndex,
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
