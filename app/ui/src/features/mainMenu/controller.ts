/**
 * Opening the main menu: the hamburger popup (New UI) or one menu of the menu bar
 * ("Merged with main toolbar" / "Show in separate toolbar"). Both render through the context
 * menu host; keys are handled there while the menu is open (the keymap is suspended).
 */
import { createSignal } from "solid-js";
import { closeContextMenu, openMenu, showResolvedMenu, type ResolvedItem } from "@/features/contextMenu/state";
import { repoAvailable } from "@/features/git/store";
import { isPluginActive } from "@/state/project";
import { getSetting, settingsState } from "@/state/settings";
import { loadMainMenu } from "./model";

export type MainMenuMode = "hamburger" | "merged" | "toolbar";

/** `appearance.mainMenu`: where the main menu lives. */
export function mainMenuMode(): MainMenuMode {
  const v = getSetting<string>("appearance.mainMenu", "hamburger");
  return v === "merged" || v === "toolbar" ? v : "hamburger";
}

export const HAMBURGER_CLASS = "main-menu-popup";
export const DROPDOWN_CLASS = "main-menu-dropdown";

const [topMenus, setTopMenus] = createSignal<ResolvedItem[]>([]);
/** Index of the open menu bar menu, -1 when none is open. */
const [barIndex, setBarIndex] = createSignal(-1);
export { topMenus, barIndex };

export const isHamburgerOpen = (): boolean => openMenu()?.class === HAMBURGER_CLASS;

/** Resolve the menu for the current project (active plugins, Git or not). */
export async function resolveMenus(): Promise<ResolvedItem[]> {
  const plugins = settingsState.plugins.filter((p) => isPluginActive(p.id));
  const menus = await loadMainMenu({ git: repoAvailable(), plugins });
  setTopMenus(menus);
  return menus;
}

/** Give the focus back to where it was once no menu is open any more. */
function focusRestorer(): () => void {
  const before = document.activeElement as HTMLElement | null;
  return () =>
    queueMicrotask(() => {
      if (!openMenu() && before?.isConnected) before.focus({ preventScroll: true });
    });
}

let request = 0;

/** Open the hamburger menu below `anchor` (the ☰ button). */
export async function openHamburger(anchor: Element | null, byKey = false): Promise<void> {
  const seq = ++request;
  const menus = await resolveMenus();
  if (seq !== request) return;
  const r = anchor?.getBoundingClientRect();
  showResolvedMenu(menus, r ? r.left : 8, r ? r.bottom + 4 : 40, {
    class: HAMBURGER_CLASS,
    crossTopLevel: true,
    selectFirst: byKey,
    keepOpenOn: "[data-main-menu-button]",
    onClose: focusRestorer(),
  });
}

/**
 * Open menu `index` of the menu bar (wrapping around). Switching between menus reuses the
 * menus resolved for the first one.
 */
export async function openBarMenu(index: number, byKey = false, reuse = false): Promise<void> {
  const seq = ++request;
  const menus = reuse && topMenus().length ? topMenus() : await resolveMenus();
  if (seq !== request || !menus.length) return;
  const i = ((index % menus.length) + menus.length) % menus.length;
  const label = document.querySelector(`[data-main-menu-index="${i}"]`);
  const r = label?.getBoundingClientRect();
  setBarIndex(i);
  showResolvedMenu(menus[i].items ?? [], r ? r.left : 8, r ? r.bottom + 2 : 40, {
    class: DROPDOWN_CLASS,
    selectFirst: byKey,
    keepOpenOn: ".main-menu-bar",
    onCross: (dir) => void openBarMenu(i + dir, true, true),
    onClose: (() => {
      const restore = focusRestorer();
      return () => {
        if (barIndex() === i) setBarIndex(-1);
        restore();
      };
    })(),
  });
}

/** Alt+\ (MainMenuButton.ShowMenu): open the main menu from the keyboard. */
export function showMainMenu(): void {
  if (openMenu()) closeContextMenu();
  if (mainMenuMode() === "hamburger") void openHamburger(document.querySelector("[data-main-menu-button]"), true);
  else void openBarMenu(0, true);
}
