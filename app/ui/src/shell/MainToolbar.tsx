import { ChevronDown, GitBranch, Search, Settings, Sparkles, Hammer, Sun, Moon } from "lucide-solid";
import { RunWidget } from "@/features/run/RunWidget";
import { isCollabGuest, workspace } from "@/state/workspace";
import { isDarkTheme, theme, toggleTheme } from "@/state/theme";
import { showToolWindow } from "@/state/layout";
import { branchLabel } from "@/state/vcs";
import { runCommand } from "@/state/commands";
import { For, Show } from "solid-js";
import { Dynamic } from "solid-js/web";
import { toolbarItems } from "@/features/toolbarItems";
import { chrome, dragRegion, drawsControls } from "./window/chrome";
import { WindowControls } from "./window/WindowControls";
import { MainMenuButton, SeparateMainMenuBar } from "@/features/mainMenu/MainMenuButton";
import { t } from "@/i18n";
import { UiIcon } from "@/features/iconThemes/UiIcon";

/**
 * Top toolbar, laid out like PHPStorm's New UI main toolbar. It is also the window's title
 * bar: empty areas move the window, a double click maximizes it, and Forge's window controls
 * sit at its ends (traffic lights at the left with the macOS style).
 */
export function MainToolbar() {
  const initials = () =>
    (workspace()?.name ?? "F")
      .split(/[-_ ]/)
      .map((p) => p[0])
      .join("")
      .slice(0, 2)
      .toUpperCase();
  return (
    <>
    <SeparateMainMenuBar />
    <header class="main-toolbar" aria-label={t("a11y.mainToolbar")} onMouseDown={dragRegion.onMouseDown}>
      <Show when={drawsControls() && chrome()!.buttons.left.length > 0}>
        <WindowControls style={chrome()!.style} buttons={chrome()!.buttons.left} side="left" />
      </Show>
      <div class="toolbar-left">
        <MainMenuButton />
        <button class="pill-btn project-widget" title={t("toolbar.openProject")} onClick={() => runCommand("workspace.open")}>
          <span class="project-badge" aria-hidden="true">{initials()}</span>
          <span class="project-name">{workspace()?.name ?? t("toolbar.noProject")}</span>
          <Show when={workspace()?.collab}>
            {(c) => (
              <span class="cwm-guest-tag">
                — <b>{c().host}</b> (Code With Me)
              </span>
            )}
          </Show>
          <ChevronDown size={14} class="muted" />
        </button>
        <button class="pill-btn" title={t("toolbar.gitBranches")} onClick={() => runCommand("git.branches")}>
          <GitBranch size={15} class="muted" />
          <span>{branchLabel() || t("toolbar.versionControl")}</span>
          <ChevronDown size={14} class="muted" />
        </button>
      </div>
      <div class="toolbar-center" />
      <div class="toolbar-right">
        <button class="icon-btn" title={t("toolbar.buildProject", { shortcut: "Ctrl+F9" })}>
          <UiIcon id="action.build" size={16}>
            <Hammer size={16} />
          </UiIcon>
        </button>
        <For each={toolbarItems()}>{(item) => <Dynamic component={item.component} />}</For>
        <RunWidget />
        <div class="toolbar-sep" />
        <Show when={!isCollabGuest()}>
          <button class="icon-btn" title={t("toolbar.aiChat")} onClick={() => showToolWindow("ai-chat")}>
            <UiIcon id="action.aiChat" size={16}>
              <Sparkles size={16} />
            </UiIcon>
          </button>
        </Show>
        <button class="icon-btn" title={t("toolbar.searchEverywhere", { shortcut: "Double Shift" })} onClick={() => runCommand("search.everywhere")}>
          <UiIcon id="action.search" size={16}>
            <Search size={16} />
          </UiIcon>
        </button>
        <button class="icon-btn" title={t("toolbar.toggleTheme")} onClick={toggleTheme}>
          {isDarkTheme(theme()) ? <Sun size={16} /> : <Moon size={16} />}
        </button>
        <button class="icon-btn" title={t("toolbar.settings", { shortcut: "Ctrl+Alt+S" })} onClick={() => runCommand("settings.open")}>
          <UiIcon id="action.settings" size={16}>
            <Settings size={16} />
          </UiIcon>
        </button>
      </div>
      <Show when={drawsControls() && chrome()!.buttons.right.length > 0}>
        <WindowControls style={chrome()!.style} buttons={chrome()!.buttons.right} side="right" />
      </Show>
    </header>
    </>
  );
}
