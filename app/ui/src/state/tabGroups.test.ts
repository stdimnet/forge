import { describe, expect, it } from "vitest";
import {
  activateTab,
  activeTabId,
  allOf,
  closeTabs,
  cycle,
  DEFAULT_POLICY,
  moveToOpposite,
  neighbourGroup,
  newLayout,
  openIds,
  openTab,
  othersOf,
  renameId,
  sideOf,
  split,
  togglePin,
  unsplit,
  type TabLayout,
  type TabPolicy,
} from "./tabGroups";

const open = (l: TabLayout, ...ids: string[]) => ids.reduce((acc, id) => openTab(acc, id).layout, l);
const tabs = (l: TabLayout, i = 0) => l.groups[i].tabs;

describe("tab groups", () => {
  it("opens new tabs after the active one and activates them", () => {
    let l = open(newLayout(), "a", "b", "c");
    l = activateTab(l, 1, "a");
    l = openTab(l, "d").layout;
    expect(tabs(l)).toEqual(["a", "d", "b", "c"]);
    expect(activeTabId(l)).toBe("d");
  });

  it("activates the most recently used tab after closing the active one", () => {
    let l = open(newLayout(), "a", "b", "c");
    l = activateTab(l, 1, "a");
    l = activateTab(l, 1, "c");
    l = closeTabs(l, 1, ["c"]);
    expect(activeTabId(l)).toBe("a");
    const right: TabPolicy = { ...DEFAULT_POLICY, activateOnClose: "right" };
    l = open(newLayout(), "a", "b", "c");
    l = closeTabs(activateTab(l, 1, "b"), 1, ["b"], right);
    expect(activeTabId(l)).toBe("c");
    const left: TabPolicy = { ...DEFAULT_POLICY, activateOnClose: "left" };
    l = open(newLayout(), "a", "b", "c");
    l = closeTabs(activateTab(l, 1, "b"), 1, ["b"], left);
    expect(activeTabId(l)).toBe("a");
  });

  it("keeps pinned tabs first and on Close Others / Close All", () => {
    let l = open(newLayout(), "a", "b", "c", "d");
    l = togglePin(l, "c");
    expect(tabs(l)).toEqual(["c", "a", "b", "d"]);
    expect(othersOf(l, 1, "b")).toEqual(["a", "d"]);
    expect(allOf(l, 1)).toEqual(["a", "b", "d"]);
    expect(sideOf(l, 1, "b", "left")).toEqual(["a"]);
    expect(sideOf(l, 1, "b", "right")).toEqual(["d"]);
    l = closeTabs(l, 1, allOf(l, 1));
    expect(tabs(l)).toEqual(["c"]);
    l = openTab(l, "e").layout;
    expect(tabs(l)).toEqual(["c", "e"]);
    l = togglePin(l, "c");
    expect(l.pinned).toEqual([]);
  });

  it("closes the least recently used unmodified tab when the limit is reached", () => {
    const dirty = new Set(["a"]);
    const policy: TabPolicy = { ...DEFAULT_POLICY, limit: 3, isDirty: (id) => dirty.has(id) };
    let l = newLayout();
    for (const id of ["a", "b", "c"]) l = openTab(l, id, { policy }).layout;
    l = togglePin(l, "c");
    const r = openTab(l, "d", { policy });
    // "a" is older but modified, "c" is pinned: "b" goes.
    expect(r.evicted).toEqual(["b"]);
    expect(tabs(r.layout)).toEqual(["c", "d", "a"]);
    const lru = openTab(r.layout, "e", { policy: { ...policy, closeUnmodifiedFirst: false } });
    expect(lru.evicted).toEqual(["a"]);
  });

  it("splits, moves tabs to the opposite group and unsplits", () => {
    let l = open(newLayout(), "a", "b");
    l = split(l, "vertical");
    expect(l.groups.map((g) => g.tabs)).toEqual([["a", "b"], ["b"]]);
    expect(l.activeGroup).toBe(l.groups[1].id);
    // The same file stays open in both groups.
    expect([...openIds(l)]).toEqual(["a", "b"]);
    l = moveToOpposite(l, l.groups[0].id, "a");
    expect(l.groups.map((g) => g.tabs)).toEqual([["b"], ["b", "a"]]);
    expect(activeTabId(l)).toBe("a");
    l = unsplit(l);
    expect(l.groups).toHaveLength(1);
    expect(tabs(l)).toEqual(["b", "a"]);
    expect(activeTabId(l)).toBe("a");
  });

  it("removes a group when its last tab closes", () => {
    let l = split(open(newLayout(), "a"), "horizontal");
    expect(l.orientation).toBe("horizontal");
    const second = l.groups[1].id;
    l = closeTabs(l, second, ["a"]);
    expect(l.groups).toHaveLength(1);
    expect(l.activeGroup).toBe(l.groups[0].id);
    // Closing a file everywhere (null group) leaves one empty group.
    l = closeTabs(split(l, "vertical"), null, ["a"]);
    expect(l.groups).toHaveLength(1);
    expect(tabs(l)).toEqual([]);
  });

  it("moving the only tab of a group keeps the pin and drops the group", () => {
    let l = togglePin(open(newLayout(), "a", "b"), "b");
    l = split(l, "vertical", 1, "a");
    l = moveToOpposite(l, l.groups[0].id, "b");
    expect(l.groups[0].tabs).toEqual(["a"]);
    expect(l.groups[1].tabs[0]).toBe("b");
    expect(l.pinned).toEqual(["b"]);
  });

  it("renames ids and cycles tabs", () => {
    let l = open(newLayout(), "a", "b", "c");
    l = renameId(togglePin(l, "b"), "b", "B");
    expect(tabs(l)).toEqual(["B", "a", "c"]);
    expect(l.pinned).toEqual(["B"]);
    expect(cycle(l, 1, 1)).toBe("B");
    expect(cycle(l, 1, -1)).toBe("a");
  });

  it("closes tabs to the left and right but keeps pinned ones", () => {
    const l = togglePin(open(newLayout(), "a", "b", "c", "d", "e"), "b");
    // Pinned tabs move to the front: b, a, c, d, e.
    expect(sideOf(l, 1, "d", "left")).toEqual(["a", "c"]);
    expect(sideOf(l, 1, "c", "right")).toEqual(["d", "e"]);
    expect(sideOf(l, 1, "missing", "right")).toEqual([]);
  });

  it("activates the left or right neighbour per closing policy", () => {
    const base = activateTab(open(newLayout(), "a", "b", "c"), 1, "b");
    const left: TabPolicy = { ...DEFAULT_POLICY, activateOnClose: "left" };
    const right: TabPolicy = { ...DEFAULT_POLICY, activateOnClose: "right" };
    expect(activeTabId(closeTabs(base, 1, ["b"], left))).toBe("a");
    expect(activeTabId(closeTabs(base, 1, ["b"], right))).toBe("c");
  });

  it("splits down, walks the splitters and changes nothing without a tab", () => {
    let l = open(newLayout(), "a");
    l = split(l, "horizontal");
    expect(l.orientation).toBe("horizontal");
    expect(l.groups.map((g) => g.tabs)).toEqual([["a"], ["a"]]);
    expect(neighbourGroup(l, 1)?.id).toBe(l.groups[0].id);
    expect(split(newLayout(), "vertical")).toEqual(newLayout());
    // Unsplit merges into the neighbour and keeps one copy of each tab.
    expect(unsplit(l, l.groups[1].id).groups.map((g) => g.tabs)).toEqual([["a"]]);
  });
});
