import { ChevronDown, GitBranch, Menu, Play, Bug, Search, Settings, Sparkles, Hammer, Sun, Moon } from "lucide-solid";
import { workspace } from "@/state/workspace";
import { theme, toggleTheme } from "@/state/theme";
import { showToolWindow } from "@/state/layout";
import { currentBranch } from "@/state/vcs";
import { runCommand } from "@/state/commands";

/** Top toolbar, laid out like PHPStorm's New UI main toolbar. */
export function MainToolbar() {
  const initials = () =>
    (workspace()?.name ?? "F")
      .split(/[-_ ]/)
      .map((p) => p[0])
      .join("")
      .slice(0, 2)
      .toUpperCase();
  return (
    <header class="main-toolbar" data-tauri-drag-region>
      <div class="toolbar-left">
        <button class="icon-btn" title="Main Menu (Alt+\\)">
          <Menu size={17} />
        </button>
        <button class="pill-btn project-widget" title="Open Project" onClick={() => runCommand("workspace.open")}>
          <span class="project-badge">{initials()}</span>
          <span class="project-name">{workspace()?.name ?? "No project"}</span>
          <ChevronDown size={14} class="muted" />
        </button>
        <button class="pill-btn" title="Git Branches" onClick={() => runCommand("git.branches")}>
          <GitBranch size={15} class="muted" />
          <span>{currentBranch()}</span>
          <ChevronDown size={14} class="muted" />
        </button>
      </div>
      <div class="toolbar-center" data-tauri-drag-region />
      <div class="toolbar-right">
        <button class="icon-btn" title="Build Project (Ctrl+F9)">
          <Hammer size={16} />
        </button>
        <button class="pill-btn run-config" title="Run/Debug Configurations" onClick={() => runCommand("run.editConfigurations")}>
          <span class="run-config-icon">P</span>
          <span>PHPUnit: tests</span>
          <ChevronDown size={14} class="muted" />
        </button>
        <button class="icon-btn run-btn" title="Run (Shift+F10)" onClick={() => runCommand("run.run")}>
          <Play size={16} fill="currentColor" />
        </button>
        <button class="icon-btn debug-btn" title="Debug (Shift+F9)" onClick={() => runCommand("run.debug")}>
          <Bug size={16} />
        </button>
        <div class="toolbar-sep" />
        <button class="icon-btn" title="AI Chat" onClick={() => showToolWindow("right", "ai-chat")}>
          <Sparkles size={16} />
        </button>
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
    </header>
  );
}
