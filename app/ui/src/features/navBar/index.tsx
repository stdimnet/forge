/**
 * Navigation Bar (View | Appearance | Navigation Bar): Top / In Status Bar / Don't Show,
 * Members in Navigation Bar, and Jump to Navigation Bar (Alt+Home), which focuses the bar
 * or shows it as a popup above the editor while it is hidden. The bar itself is
 * `NavigationBar.tsx`; the editor area and the status bar render it.
 */
import { Show } from "solid-js";
import { registerCommand } from "@/state/commands";
import { activeGroupId, editor } from "@/state/editor";
import { registerKeymapActions } from "@/keymap";
import { registerMainMenuToggle } from "@/features/mainMenu/model";
import { NavigationBar } from "./NavigationBar";
import {
  floatingNavBar,
  navBarLocation,
  navBarMembers,
  requestNavBarFocus,
  setFloatingNavBar,
  setNavBarLocation,
  setNavBarMembers,
  type NavBarLocation,
} from "./model";
import "./navBar.css";

export { NavigationBar } from "./NavigationBar";
export { navBarLocation } from "./model";

registerKeymapActions([
  { id: "NavBarLocationTop", title: "Navigation Bar: Top", group: "Main Menu/View", shortcuts: {} },
  { id: "NavBarLocationBottom", title: "Navigation Bar: In Status Bar", group: "Main Menu/View", shortcuts: {} },
  { id: "NavBarLocationHide", title: "Navigation Bar: Don't Show", group: "Main Menu/View", shortcuts: {} },
  { id: "ShowMembersInNavigationBar", title: "Members in Navigation Bar", group: "Main Menu/View", shortcuts: {} },
]);

const LOCATIONS: [string, NavBarLocation, string][] = [
  ["NavBarLocationTop", "top", "Navigation Bar: Top"],
  ["NavBarLocationBottom", "statusBar", "Navigation Bar: In Status Bar"],
  ["NavBarLocationHide", "hidden", "Navigation Bar: Don't Show"],
];
for (const [id, location, title] of LOCATIONS) {
  registerCommand({ id, title, category: "View", run: () => void setNavBarLocation(location) });
  registerMainMenuToggle(id, () => navBarLocation() === location);
}

registerCommand({
  id: "ShowMembersInNavigationBar",
  title: "Members in Navigation Bar",
  category: "View",
  run: () => void setNavBarMembers(!navBarMembers()),
});
registerMainMenuToggle("ShowMembersInNavigationBar", navBarMembers);

registerCommand({
  id: "ShowNavBar",
  title: "Jump to Navigation Bar",
  category: "View",
  run: () => {
    const group = activeGroupId();
    switch (navBarLocation()) {
      case "top":
        // Tool tabs (custom tabs) have no bar above them: fall back to the status bar.
        if (document.querySelector(`.navbar-top[data-group="${group}"]`)) requestNavBarFocus("top", group);
        else requestNavBarFocus("statusBar");
        break;
      case "statusBar":
        requestNavBarFocus("statusBar");
        break;
      default:
        setFloatingNavBar({ group });
        requestNavBarFocus("floating", group);
    }
  },
});

/** Over the top of the editor group (below its tabs). */
function hostPosition(group: number): Record<string, string> {
  const el = document.querySelector(`[data-editor-group="${group}"]`);
  const tabs = el?.querySelector(".editor-tabs")?.getBoundingClientRect();
  const r = el?.getBoundingClientRect();
  if (!r) return { left: "50%", top: "80px", transform: "translateX(-50%)" };
  return { left: `${r.left + 12}px`, top: `${(tabs?.bottom ?? r.top) + 6}px`, "max-width": `${Math.max(240, r.width - 24)}px` };
}

/** Alt+Home while the bar is hidden: the bar as a popup at the top of the editor area. */
export function FloatingNavBar() {
  const tab = () => {
    const g = floatingNavBar()?.group;
    const id = editor.layout.groups.find((x) => x.id === g)?.active;
    return editor.tabs.find((t) => t.path === id);
  };
  return (
    <Show when={floatingNavBar()}>
      {(f) => (
        <div class="navbar-floating-host" style={hostPosition(f().group)}>
          <NavigationBar placement="floating" group={f().group} tab={tab} />
        </div>
      )}
    </Show>
  );
}
