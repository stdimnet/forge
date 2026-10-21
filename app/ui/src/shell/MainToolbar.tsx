import { ChevronDown, GitBranch, Search, Settings, Sparkles, Hammer, Sun, Moon } from "lucide-solid";
import { RunWidget } from "@/features/run/RunWidget";
import { isCollabGuest, workspace } from "@/state/workspace";
import { theme, toggleTheme } from "@/state/theme";
import { showToolWindow } from "@/state/layout";
import { currentBranch } from "@/state/vcs";
import { runCommand } from "@/state/commands";
import { For, Show } from "solid-js";
import { Dynamic } from "solid-js/web";
import { toolbarItems } from "@/features/toolbarItems";
import { chrome, dragRegion, drawsControls } from "./window/chrome";
import { WindowControls } from "./window/WindowControls";
import { MainMenuButton, SeparateMainMenuBar } from "@/features/mainMenu/MainMenuButton";

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
    <header class="main-toolbar" onMouseDown={dragRegion.onMouseDown}>
      <Show when={drawsControls() && chrome()!.buttons.left.length > 0}>
        <WindowControls style={chrome()!.style} buttons={chrome()!.buttons.left} side="left" />
      </Show>
      <div class="toolbar-left">
        <MainMenuButton />
        <button class="pill-btn project-widget" title="Open Project" onClick={() => runCommand("workspace.open")}>
          <span class="project-badge">{initials()}</span>
          <span class="project-name">{workspace()?.name ?? "No project"}</span>
          <Show when={workspace()?.collab}>
            {(c) => (
              <span class="cwm-guest-tag">
                — <b>{c().host}</b> (Code With Me)
              </span>
            )}
          </Show>
          <ChevronDown size={14} class="muted" />
        </button>
        <button class="pill-btn" title="Git Branches" onClick={() => runCommand("git.branches")}>
          <GitBranch size={15} class="muted" />
          <span>{currentBranch()}</span>
          <ChevronDown size={14} class="muted" />
        </button>
      </div>
      <div class="toolbar-center" />
      <div class="toolbar-right">
        <button class="icon-btn" title="Build Project (Ctrl+F9)">
          <Hammer size={16} />
        </button>
        <For each={toolbarItems()}>{(item) => <Dynamic component={item.component} />}</For>
        <RunWidget />
        <div class="toolbar-sep" />
        <Show when={!isCollabGuest()}>
          <button class="icon-btn" title="AI Chat" onClick={() => showToolWindow("right", "ai-chat")}>
            <Sparkles size={16} />
          </button>
        </Show>
        <button class="icon-btn" title="Search Everywhere (Double Shift)" onClick={() => runCommand("search.everywhere")}>
          <Search size={16} />
        </button>
        <button class="icon-btn" title="Toggle theme" onClick={toggleTheme}>
          {theme() === "dark" ? <Sun size={16} /> : <Moon size={16} />}
        </button>
        <button class="icon-btn" title="Settings (Ctrl+Alt+S)" onClick={() => runCommand("settings.open")}>
          <Settings size={16} />
        </button>
      </div>
      <Show when={drawsControls() && chrome()!.buttons.right.length > 0}>
        <WindowControls style={chrome()!.style} buttons={chrome()!.buttons.right} side="right" />
      </Show>
    </header>
    </>
  );
}
