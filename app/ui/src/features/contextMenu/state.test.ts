import { beforeAll, describe, expect, it } from "vitest";
import "@/mock/settingsMock";
import "@/mock/keymapMock";
import { loadKeymapCore } from "@/keymap/core";
import { startKeymap } from "@/keymap/store";
import { registerCommand } from "@/state/commands";
import { clampToViewport } from "./ContextMenu";
import { action, resolveEntries, separator, submenu } from "./state";

beforeAll(async () => {
  // Labels and shortcuts come from the Rust keymap (through WebAssembly in tests).
  await Promise.all([loadKeymapCore(), startKeymap()]);
  for (const id of ["$Cut", "$Copy", "ReformatCode"]) registerCommand({ id, title: id, run: () => {} });
});

describe("resolveEntries", () => {
  it("drops actions nobody implements and tidies the separators around them", () => {
    const out = resolveEntries([separator, action("FindUsages"), separator, action("$Cut"), action("$Copy"), separator, action("Generate"), separator]);
    expect(out.map((e) => (e === "separator" ? "-" : e.action))).toEqual(["$Cut", "$Copy"]);
  });

  it("takes label and shortcut from the keymap and lets entries override the label", () => {
    const [cut, reformat] = resolveEntries([action("$Cut"), action("ReformatCode", "Reformat")]);
    expect(cut).toMatchObject({ label: "Cut", shortcut: expect.stringContaining("X") });
    expect(reformat).toMatchObject({ label: "Reformat" });
  });

  it("hides submenus without available entries", () => {
    const out = resolveEntries([submenu("Refactor", [action("RenameElement")]), submenu("Code", [action("ReformatCode")])]);
    expect(out).toHaveLength(1);
    expect(out[0]).toMatchObject({ kind: "submenu", label: "Code" });
  });
});

describe("clampToViewport", () => {
  it("keeps the menu inside the viewport", () => {
    expect(clampToViewport(900, 200, 1000)).toBe(794);
    expect(clampToViewport(-20, 200, 1000)).toBe(6);
    expect(clampToViewport(100, 200, 1000)).toBe(100);
  });
});
