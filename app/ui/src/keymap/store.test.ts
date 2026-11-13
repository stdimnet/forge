import { beforeAll, beforeEach, describe, expect, it } from "vitest";
import "@/mock/settingsMock";
import "@/mock/keymapMock";
import { mockSettingValue } from "@/mock/settingsMock";
import { registerCommand } from "@/state/commands";
import { loadKeymapCore } from "./core";
import {
  actionById,
  addActionShortcut,
  allActions,
  activeKeymap,
  checkShortcut,
  deleteKeymap,
  duplicateKeymap,
  findActionsByShortcut,
  formatForKeymap,
  isModified,
  keymapId,
  keymapNameProblem,
  registerKeymapActions,
  removeActionShortcut,
  resetAction,
  resetKeymap,
  setKeymapId,
  shortcutLabel,
  shortcutsOf,
  startKeymap,
} from "./store";

/** Let queued registrations reach the backend. */
const settle = () => new Promise((r) => setTimeout(r, 20));

describe("keymap client", () => {
  beforeAll(async () => {
    await Promise.all([loadKeymapCore(), startKeymap()]);
  });
  beforeEach(async () => {
    await setKeymapId("$default");
    await resetKeymap();
  });

  it("serves the PHPStorm defaults computed in Rust", async () => {
    expect(allActions().length).toBeGreaterThan(250);
    expect(shortcutsOf("GotoAction")).toEqual(["Ctrl+Shift+A"]);
    expect(shortcutLabel("GotoAction")).toBe("Ctrl+Shift+A");
    expect(actionById("Run")?.aliases).toEqual(["run.run"]);
    await setKeymapId("macOS");
    expect(keymapId()).toBe("macOS");
    expect(shortcutLabel("GotoAction")).toBe("⇧⌘A");
    expect(formatForKeymap("Ctrl+Alt+S")).toBe("⌃⌥S");
  });

  it("stores overrides in the settings", async () => {
    await addActionShortcut("GotoClass", "Ctrl+Alt+Shift+K");
    expect(shortcutsOf("GotoClass")).toEqual(["Ctrl+N", "Ctrl+Alt+Shift+K"]);
    expect(isModified("GotoClass")).toBe(true);
    expect(mockSettingValue("keymap.overrides.default")).toEqual(["GotoClass\tCtrl+N\tCtrl+Alt+Shift+K"]);
    await removeActionShortcut("GotoClass", "ctrl+n");
    expect(shortcutsOf("GotoClass")).toEqual(["Ctrl+Alt+Shift+K"]);
    await resetAction("GotoClass");
    expect(shortcutsOf("GotoClass")).toEqual(["Ctrl+N"]);
    expect(isModified("GotoClass")).toBe(false);
  });

  it("checks conflicts and finds actions by shortcut", async () => {
    const check = await checkShortcut("ctrl+n", "GotoFile");
    expect(check.valid).toBe(true);
    expect(check.conflicts.map((c) => [c.actionId, c.kind])).toEqual([["GotoClass", "same"]]);
    expect(await findActionsByShortcut("Ctrl+N")).toContain("GotoClass");
  });

  it("accepts plugin actions and lists bare commands", async () => {
    registerKeymapActions([
      { id: "php.RunComposer", title: "Run Composer", group: "Plugins/PHP", shortcuts: { $default: ["Ctrl+Alt+Shift+R"] } },
    ]);
    registerCommand({ id: "demo.hello", title: "Say Hello", category: "Demo", run: () => {} });
    await settle();
    expect(actionById("php.RunComposer")?.context).toBe("any");
    expect(shortcutsOf("php.RunComposer")).toEqual(["Ctrl+Alt+Shift+R"]);
    expect(actionById("demo.hello")?.group).toBe("Plugins/Demo");
  });

  it("switches to the other bundled keymaps and makes user keymaps", async () => {
    await setKeymapId("VSCode");
    expect(keymapId()).toBe("VSCode");
    expect(shortcutsOf("GotoFile")).toEqual(["Ctrl+P"]);
    expect(activeKeymap()?.parent).toBe("$default");
    await duplicateKeymap("Mine");
    expect(keymapId()).toBe("Mine");
    expect(activeKeymap()?.user).toBe(true);
    expect(shortcutsOf("GotoFile")).toEqual(["Ctrl+P"]);
    expect(keymapNameProblem("Mine")).toBe('A keymap named "Mine" already exists');
    expect(keymapNameProblem("VS Code")).toMatch(/already exists/);
    expect(keymapNameProblem("Mine", "Mine")).toBeNull();
    expect(keymapNameProblem("  ")).toBe("Enter a name");
    expect(keymapNameProblem("a\tb")).toBe("The name cannot contain tabs or line breaks");
    await deleteKeymap("Mine");
    expect(keymapId()).toBe("VSCode");
    expect(mockSettingValue("keymap.custom")).toEqual([]);
  });

  it("lists mouse shortcuts of the PHPStorm keymap", () => {
    expect(shortcutsOf("GotoDeclaration")).toContain("Ctrl+Button1 Click");
    expect(actionById("GotoDeclaration")?.display).toContain("Ctrl+Button1 Click");
  });
});
