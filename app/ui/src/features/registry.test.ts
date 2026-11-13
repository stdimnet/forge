import { describe, expect, it } from "vitest";
import { Folder } from "lucide-solid";
import { registerToolWindow, toolWindowsFor } from "./registry";

describe("tool window registry", () => {
  it("replaces windows with the same id and sorts by order", () => {
    const component = () => null;
    registerToolWindow({ id: "b", title: "B", icon: Folder, anchor: "left", order: 2, component });
    registerToolWindow({ id: "a", title: "A", icon: Folder, anchor: "left", order: 1, component });
    registerToolWindow({ id: "b", title: "B2", icon: Folder, anchor: "left", order: 0, component });
    expect(toolWindowsFor("left").map((t) => t.title)).toEqual(["B2", "A"]);
  });

  it("never lets a placeholder replace the real window, whatever registers first", () => {
    const component = () => null;
    registerToolWindow({ id: "late", title: "Real", icon: Folder, anchor: "right", order: 1, component });
    registerToolWindow({ id: "late", title: "Soon", icon: Folder, anchor: "right", order: 1, component, placeholder: true });
    registerToolWindow({ id: "early", title: "Soon", icon: Folder, anchor: "right", order: 2, component, placeholder: true });
    registerToolWindow({ id: "early", title: "Real", icon: Folder, anchor: "right", order: 2, component });
    expect(toolWindowsFor("right").map((t) => `${t.id}:${t.title}`)).toEqual(["late:Real", "early:Real"]);
  });
});
