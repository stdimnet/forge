/**
 * The main menu in the main toolbar: the ☰ button (New UI) or, with "Merged with main
 * toolbar", the menu names inline; plus the classic menu bar above the toolbar
 * ("Show in separate toolbar").
 */
import { For, Match, Show, Switch, createEffect, on } from "solid-js";
import { Menu } from "lucide-solid";
import { closeContextMenu } from "@/features/contextMenu/state";
import { MnemonicLabel } from "@/features/contextMenu/ContextMenu";
import { repoAvailable } from "@/features/git/store";
import { shortcutLabel } from "@/keymap";
import { settingsState } from "@/state/settings";
import { dragRegion } from "@/shell/window/chrome";
import { barIndex, isHamburgerOpen, mainMenuMode, openBarMenu, openHamburger, resolveMenus, topMenus } from "./controller";

/** Menu names in a row: File Edit View … */
export function MainMenuBar(props: { inline?: boolean }) {
  // Git or VCS, Tools contributions of the project's plugins: refresh the names.
  createEffect(
    on([repoAvailable, () => settingsState.plugins], () => void resolveMenus().catch(() => {})),
  );
  return (
    <div class="main-menu-bar" classList={{ inline: !!props.inline }} role="menubar" onMouseDown={props.inline ? undefined : dragRegion.onMouseDown}>
      <For each={topMenus()}>
        {(menu, i) => (
          <button
            class="main-menu-bar-item"
            classList={{ open: barIndex() === i() }}
            role="menuitem"
            aria-haspopup="menu"
            aria-expanded={barIndex() === i()}
            data-main-menu-index={i()}
            // Keep the focus where it is; the menu takes it when it opens.
            onMouseDown={(e) => e.preventDefault()}
            onClick={() => (barIndex() === i() ? closeContextMenu() : void openBarMenu(i()))}
            onMouseEnter={() => {
              if (barIndex() >= 0 && barIndex() !== i()) void openBarMenu(i(), false, true);
            }}
          >
            <MnemonicLabel label={menu.label} mnemonic={menu.mnemonic} />
          </button>
        )}
      </For>
    </div>
  );
}

/** Left end of the main toolbar: ☰ or the inline menu names. */
export function MainMenuButton() {
  const title = () => {
    const sc = shortcutLabel("MainMenuButton.ShowMenu");
    return sc ? `Main Menu (${sc})` : "Main Menu";
  };
  return (
    <Switch>
      <Match when={mainMenuMode() === "hamburger"}>
        <button
          class="icon-btn"
          classList={{ active: isHamburgerOpen() }}
          title={title()}
          aria-haspopup="menu"
          aria-expanded={isHamburgerOpen()}
          data-main-menu-button
          onMouseDown={(e) => e.preventDefault()}
          onClick={(e) => (isHamburgerOpen() ? closeContextMenu() : void openHamburger(e.currentTarget))}
        >
          <Menu size={17} />
        </button>
      </Match>
      <Match when={mainMenuMode() === "merged"}>
        <MainMenuBar inline />
      </Match>
    </Switch>
  );
}

/** The classic menu bar above the main toolbar, shown with "Show in separate toolbar". */
export function SeparateMainMenuBar() {
  return (
    <Show when={mainMenuMode() === "toolbar"}>
      <MainMenuBar />
    </Show>
  );
}
