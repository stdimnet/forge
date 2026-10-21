import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { render } from "solid-js/web";
import "@/mock/settingsMock";
import "@/mock/keymapMock";
import type { PluginInfo } from "@/ipc/settings";
import { loadKeymapCore } from "@/keymap/core";
import { startKeymap } from "@/keymap/store";
import { registerCommand } from "@/state/commands";
import { ContextMenuHost, mnemonicMatches } from "@/features/contextMenu/ContextMenu";
import { closeContextMenu, openMenu, showResolvedMenu, type ResolvedItem } from "@/features/contextMenu/state";
import {
  action,
  buildRequest,
  group,
  loadMainMenu,
  registerMainMenuDynamic,
  registerMainMenuEntries,
  registerMainMenuToggle,
  toggle,
} from "./model";

const settings = vi.fn();
const recent = vi.fn();
const plugin = (contributions: PluginInfo["contributions"]): PluginInfo =>
  ({ id: "forge.laravel", name: "Laravel", enabled: true, contributions }) as PluginInfo;

beforeAll(async () => {
  // Labels, shortcuts and enabled states come from the Rust keymap (WebAssembly in tests).
  await Promise.all([loadKeymapCore(), startKeymap()]);
  registerCommand({ id: "ShowSettings", title: "Settings…", run: settings });
  registerCommand({ id: "SaveAll", title: "Save All", run: () => {} });
  registerCommand({ id: "ToggleCompactMode", title: "Compact Mode", run: () => {} });
  registerCommand({ id: "Laravel.RunArtisan", title: "Run Artisan Command…", run: () => {} });
  registerMainMenuToggle("ToggleCompactMode", () => true);
  registerMainMenuDynamic("recentProjects", () => [{ key: "/work/shop", label: "shop", detail: "/work/shop", run: recent }]);
  registerMainMenuEntries("Help", [action("SaveAll", "Save Everything")], { when: () => false });
  // Let the batched command registration reach the keymap backend.
  await new Promise((r) => setTimeout(r, 0));
  await startKeymap();
});

afterEach(() => closeContextMenu());

const sub = (items: ResolvedItem[] | undefined, label: string): ResolvedItem => {
  const found = items?.find((i) => i !== ("separator" as unknown) && i.label === label);
  if (!found) throw new Error(`${label} missing`);
  return found;
};

describe("main menu model", () => {
  it("resolves PHPStorm's menus with keymap labels, shortcuts and disabled entries", async () => {
    const menus = await loadMainMenu({ git: true, plugins: [] });
    expect(menus.map((m) => m.label)).toEqual([
      "File", "Edit", "View", "Navigate", "Code", "Refactor", "Run", "Tools", "Git", "Window", "Help",
    ]);
    const file = menus[0].items as ResolvedItem[];
    const settingsItem = sub(file, "Settings...");
    expect(settingsItem).toMatchObject({ disabled: false, shortcut: "Ctrl+Alt+S", mnemonic: 0 });
    // Nobody implements Invalidate Caches yet: shown, but disabled.
    expect(sub(file, "Invalidate Caches...").disabled).toBe(true);
    const appearance = sub(sub(menus, "View").items as ResolvedItem[], "Appearance").items as ResolvedItem[];
    expect(sub(appearance, "Compact Mode")).toMatchObject({ checkable: true, checked: true, disabled: false });
  });

  it("fills dynamic lists and runs their handlers", async () => {
    const menus = await loadMainMenu({ git: false, plugins: [] });
    expect(menus[8].label).toBe("VCS");
    const recentProjects = sub(menus[0].items as ResolvedItem[], "Recent Projects").items as ResolvedItem[];
    const shop = sub(recentProjects, "shop");
    expect(shop).toMatchObject({ detail: "/work/shop", disabled: false });
    shop.run!();
    expect(recent).toHaveBeenCalled();
  });

  it("adds manifest and UI contributions of active plugins", async () => {
    const laravel = plugin({
      mainMenu: [{ path: "Tools/_Laravel", items: [action("Laravel.RunArtisan"), toggle("Laravel.Unknown", "Unknown")] }],
    } as unknown as PluginInfo["contributions"]);
    const { request } = await buildRequest({ git: true, plugins: [laravel] });
    // The `when: () => false` contribution is left out.
    expect(request.contributions.map((c) => c.path)).toEqual(["Tools/_Laravel"]);
    expect(request.checked).toContain("ToggleCompactMode");
    const menus = await loadMainMenu({ git: true, plugins: [laravel] });
    const tools = sub(menus, "Tools").items as ResolvedItem[];
    const entry = sub(tools, "Laravel");
    expect(entry.mnemonic).toBe(0);
    expect((entry.items as ResolvedItem[]).map((i) => [i.label, i.disabled])).toEqual([
      ["Run Artisan Command...", false],
      ["Unknown", true],
    ]);
    expect(group("X", []).type).toBe("group");
  });
});

describe("main menu rendering and keys", () => {
  it("finds items by mnemonic", async () => {
    const menus = await loadMainMenu({ git: true, plugins: [] });
    expect(mnemonicMatches(menus, "v").map((i) => menus[i].label)).toEqual(["View"]);
    expect(mnemonicMatches(menus, "u").map((i) => menus[i].label)).toEqual(["Run"]);
  });

  it("navigates with arrows and mnemonics and runs the picked action", async () => {
    const host = document.createElement("div");
    document.body.appendChild(host);
    const dispose = render(() => <ContextMenuHost />, host);
    const menus = await loadMainMenu({ git: true, plugins: [] });
    showResolvedMenu(menus, 0, 0, { crossTopLevel: true, selectFirst: true, class: "main-menu-popup" });
    await Promise.resolve();
    const root = host.querySelector<HTMLElement>(".context-menu.main-menu-popup")!;
    expect(root.querySelector(".context-menu-item.active")?.textContent).toContain("File");
    expect(root.querySelector(".context-menu-item .mnemonic")?.textContent).toBe("F");

    const press = (el: Element, key: string, code = "") =>
      el.dispatchEvent(new KeyboardEvent("keydown", { key, code, bubbles: true }));
    // Right opens File with its first item selected; "s" picks Settings… by its mnemonic.
    press(root, "ArrowRight");
    await Promise.resolve();
    const fileMenu = root.querySelector<HTMLElement>(".context-menu-item.open .context-menu")!;
    expect(fileMenu.querySelector(".context-menu-item.active")?.textContent).toContain("New");
    expect(fileMenu.querySelector(".context-menu-item.disabled")).not.toBeNull();
    press(fileMenu, "s", "KeyS");
    await Promise.resolve();
    expect(settings).toHaveBeenCalled();
    expect(openMenu()).toBeNull();
    dispose();
    host.remove();
  });

  it("moves to the next top-level menu with Right on an item without submenu", async () => {
    const host = document.createElement("div");
    document.body.appendChild(host);
    const dispose = render(() => <ContextMenuHost />, host);
    const menus = await loadMainMenu({ git: true, plugins: [] });
    showResolvedMenu(menus, 0, 0, { crossTopLevel: true, selectFirst: true });
    await Promise.resolve();
    const root = host.querySelector<HTMLElement>(".context-menu")!;
    root.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true }));
    await Promise.resolve();
    const fileMenu = root.querySelector<HTMLElement>(".context-menu-item.open .context-menu")!;
    // Down past Recent Projects to Settings… (disabled entries are skipped), then Right
    // switches to Edit.
    fileMenu.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true }));
    fileMenu.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true }));
    expect(fileMenu.querySelector(".context-menu-item.active")?.textContent).toContain("Settings");
    fileMenu.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true }));
    await new Promise((r) => setTimeout(r, 0));
    expect(root.querySelector(":scope > .context-menu-item.open")?.textContent).toContain("Edit");
    dispose();
    host.remove();
  });
});
