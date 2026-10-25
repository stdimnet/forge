import { afterEach, beforeAll, describe, expect, it } from "vitest";
import { registerMock } from "@/ipc";
import { registerToolWindow } from "@/features/registry";
import { layout, setLayout, showToolWindow } from "./layout";
import { SLOTS, type ActiveWindows } from "./toolWindowLayout";
import { appearanceMode, isFullScreen, resetAppearanceModes, toggleAppearanceMode, toggleFullScreen } from "./appearanceModes";

const Empty = () => null;
const calls: boolean[] = [];

beforeAll(() => {
  registerMock("window_set_fullscreen", ({ on }) => void calls.push(!!on));
  registerToolWindow({ id: "am-project", title: "Project", icon: Empty, anchor: "left", order: 0, component: Empty });
  registerToolWindow({ id: "am-terminal", title: "Terminal", icon: Empty, anchor: "bottom", order: 0, component: Empty });
});

afterEach(() => {
  resetAppearanceModes();
  calls.length = 0;
  setLayout("active", Object.fromEntries(SLOTS.map((s) => [s, null])) as ActiveWindows);
});

describe("appearance modes", () => {
  it("hide the tool windows and restore them when left", async () => {
    showToolWindow("am-project");
    showToolWindow("am-terminal");
    await toggleAppearanceMode("distractionFree");
    expect(appearanceMode()).toBe("distractionFree");
    expect(SLOTS.every((s) => layout.active[s] === null)).toBe(true);
    // Distraction-Free Mode stays in the window.
    expect(calls).toEqual([]);
    await toggleAppearanceMode("distractionFree");
    expect(appearanceMode()).toBeNull();
    expect(layout.active.left).toBe("am-project");
    expect(layout.active.bottom).toBe("am-terminal");
  });

  it("make Presentation and Zen Mode full screen and leave it again", async () => {
    showToolWindow("am-project");
    await toggleAppearanceMode("zen");
    expect(isFullScreen()).toBe(true);
    // Switching modes leaves Zen first: the window was not full screen before.
    await toggleAppearanceMode("presentation");
    expect(appearanceMode()).toBe("presentation");
    expect(calls).toEqual([true, false, true]);
    await toggleAppearanceMode("presentation");
    expect(isFullScreen()).toBe(false);
    expect(layout.active.left).toBe("am-project");
  });

  it("keep a full screen window full screen after the mode", async () => {
    await toggleFullScreen();
    expect(isFullScreen()).toBe(true);
    await toggleAppearanceMode("zen");
    await toggleAppearanceMode("zen");
    expect(isFullScreen()).toBe(true);
    expect(calls).toEqual([true]);
  });

  it("leave Presentation Mode when full screen is turned off", async () => {
    await toggleAppearanceMode("presentation");
    await toggleFullScreen();
    expect(appearanceMode()).toBeNull();
    expect(isFullScreen()).toBe(false);
  });
});
