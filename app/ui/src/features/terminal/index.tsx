/**
 * Terminal tool window (Alt+F12): local shell sessions on a PTY, rendered with xterm.js.
 * Replaces the `terminal` placeholder.
 */
import { SquareTerminal } from "lucide-solid";
import { registerToolWindow } from "@/features/registry";
import { registerCommand } from "@/state/commands";
import { registerKeymapActions } from "@/keymap";
import { showToolWindow } from "@/state/layout";
import { TerminalTabs, TerminalView, closeTerminalTab } from "./TerminalView";
import { activeTab, newTab, type TerminalProgram } from "./store";
import { focusSession } from "./sessions";

registerToolWindow({
  id: "terminal",
  title: "Terminal",
  icon: SquareTerminal,
  anchor: "bottom",
  order: 40,
  shortcut: "Alt+F12",
  component: TerminalView,
  headerContent: TerminalTabs,
});

/** Open a new terminal tab in `dir` (relative to the project root), e.g. "Open In > Terminal". */
export function openTerminalAt(dir?: string): void {
  const tab = newTab(dir);
  showToolWindow("bottom", "terminal");
  requestAnimationFrame(() => focusSession(tab.id));
}

/** Open a terminal tab running a program (Services: exec into a container or pod). */
export function openTerminalWith(command: TerminalProgram & { title: string }): void {
  const tab = newTab(undefined, command);
  showToolWindow("bottom", "terminal");
  requestAnimationFrame(() => focusSession(tab.id));
}

registerKeymapActions([
  {
    id: "Terminal.NewTab",
    title: "New Terminal Tab",
    group: "Plugins/Terminal",
    context: "toolwindow",
    shortcuts: { $default: ["Ctrl+Shift+T"], macOS: ["Cmd+T"] },
  },
  {
    id: "Terminal.CloseTab",
    title: "Close Terminal Tab",
    group: "Plugins/Terminal",
    context: "toolwindow",
    shortcuts: { $default: [], macOS: [] },
  },
]);

registerCommand({
  id: "terminal.open",
  title: "Terminal",
  category: "View",
  run: () => {
    showToolWindow("bottom", "terminal");
    const id = activeTab();
    if (id) requestAnimationFrame(() => focusSession(id));
  },
});
registerCommand({ id: "Terminal.NewTab", title: "New Terminal Tab", category: "Terminal", run: () => openTerminalAt() });
registerCommand({
  id: "Terminal.CloseTab",
  title: "Close Terminal Tab",
  category: "Terminal",
  run: () => {
    const id = activeTab();
    if (id) closeTerminalTab(id);
  },
});
