import { beforeEach, describe, expect, it } from "vitest";
import {
  addActionShortcut,
  allActions,
  bindings,
  isModified,
  keymapId,
  registerKeymapActions,
  removeActionShortcut,
  resetAction,
  resetKeymap,
  setKeymapId,
  shortcutLabel,
  shortcutsOf,
} from "./store";
import { registerCommand } from "@/state/commands";
import { shortcutId } from "./shortcut";

describe("keymap store", () => {
  beforeEach(() => {
    setKeymapId("$default");
    resetKeymap();
  });

  it("serves PHPStorm defaults per keymap", () => {
    expect(shortcutsOf("GotoAction")).toEqual(["Ctrl+Shift+A"]);
    expect(shortcutLabel("GotoAction")).toBe("Ctrl+Shift+A");
    setKeymapId("macOS");
    expect(keymapId()).toBe("macOS");
    expect(shortcutLabel("GotoAction")).toBe("⇧⌘A");
  });

  it("layers overrides on top and persists them", () => {
    addActionShortcut("GotoClass", "Ctrl+Alt+Shift+K");
    expect(shortcutsOf("GotoClass")).toEqual(["Ctrl+N", "Ctrl+Alt+Shift+K"]);
    expect(isModified("GotoClass")).toBe(true);
    expect(JSON.parse(localStorage.getItem("forge.keymap.overrides")!).$default.GotoClass).toHaveLength(2);
    // Overrides are per keymap.
    setKeymapId("macOS");
    expect(shortcutsOf("GotoClass")).toEqual(["Cmd+O"]);
  });

  it("does not add the same shortcut twice in another spelling", () => {
    addActionShortcut("GotoClass", "ctrl+n");
    expect(shortcutsOf("GotoClass")).toEqual(["Ctrl+N"]);
  });

  it("removes shortcuts by canonical form and resets", () => {
    removeActionShortcut("GotoClass", "ctrl+n");
    expect(shortcutsOf("GotoClass")).toEqual([]);
    resetAction("GotoClass");
    expect(shortcutsOf("GotoClass")).toEqual(["Ctrl+N"]);
    expect(isModified("GotoClass")).toBe(false);
  });

  it("feeds overrides into the engine bindings", () => {
    const has = (id: string, sc: string) => bindings().some((b) => b.actionId === id && shortcutId(b.shortcut) === sc);
    expect(has("GotoClass", "Ctrl+N")).toBe(true);
    removeActionShortcut("GotoClass", "Ctrl+N");
    expect(has("GotoClass", "Ctrl+N")).toBe(false);
  });

  it("leaves native actions out of the bindings", () => {
    expect(bindings().some((b) => b.actionId === "$Copy")).toBe(false);
  });

  it("accepts plugin actions with default shortcuts", () => {
    registerKeymapActions([
      { id: "php.RunComposer", title: "Run Composer", group: "Plugins/PHP", shortcuts: { $default: ["Ctrl+Alt+Shift+R"], macOS: ["Cmd+Alt+Shift+R"] } },
    ]);
    expect(allActions().find((a) => a.id === "php.RunComposer")?.context).toBe("any");
    expect(shortcutsOf("php.RunComposer")).toEqual(["Ctrl+Alt+Shift+R"]);
  });

  it("lists registered commands that have no keymap entry", () => {
    registerCommand({ id: "demo.hello", title: "Say Hello", category: "Demo", run: () => {} });
    expect(allActions().find((a) => a.id === "demo.hello")?.group).toBe("Plugins/Demo");
  });
});
