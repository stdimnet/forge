/**
 * Terminal tool window (Alt+F12): local shell sessions on a PTY and SSH sessions (Tools |
 * Start SSH Session…), rendered with xterm.js. Replaces the `terminal` placeholder.
 */
import { Show } from "solid-js";
import { SquareTerminal } from "lucide-solid";
import { registerToolWindow } from "@/features/registry";
import type { SshTarget } from "@/ipc/terminal";
import { registerStatusItem } from "@/features/statusItems";
import { getSetting } from "@/state/settings";
import { registerCommand } from "@/state/commands";
import { registerKeymapActions, showBalloon } from "@/keymap";
import { showToolWindow } from "@/state/layout";
import { TerminalTabs, TerminalView, closeTerminalTab } from "./TerminalView";
import { activeTab, displayTitle, newSshTab, newTab, newTabBlocked, overallProgress, progressTab, tabs, type TerminalProgram } from "./store";
import { focusSession, revealTerminalTab, setSshHostKeyResolver } from "./sessions";
import { chooseSshSession, confirmSshHostKey, setSshTabOpener } from "./ssh";
import { progressLabel } from "./osc";
import { ProgressRing } from "./ProgressRing";
import { t, t as tr } from "@/i18n";
// Terminals belong to the project: confirm running commands, keep the layout per project.
import "./projectTerminals";

/** Stripe marker while the Terminal is hidden: combined progress, else a bell dot. */
function TerminalStripeBadge() {
  const progress = () => overallProgress(tabs());
  return (
    <Show when={progress()} fallback={<Show when={tabs().some((t) => t.bell)}>{<span class="term-tab-bell" />}</Show>}>
      {(p) => <ProgressRing progress={p()} size={13} />}
    </Show>
  );
}

/** Status bar: progress of a terminal program, e.g. "Claude Code: 42%"; click shows its tab. */
function TerminalProgressStatus() {
  const tab = () => (getSetting<boolean>("terminal.showProgressInStatusBar", true) ? progressTab(tabs(), activeTab()) : undefined);
  return (
    <Show when={tab()}>
      {(t) => (
        <button
          class="status-item term-status"
          classList={{ [t().progress!.state]: true }}
          data-status="terminal-progress"
          title={tr("tools.terminal.clickToShowTheTerminal", { value: displayTitle(t()), value2: progressLabel(t().progress!) })}
          onClick={() => revealTerminalTab(t().id)}
        >
          <ProgressRing progress={t().progress!} size={12} />
          <span class="term-status-text">{displayTitle(t())}:</span>
          <span class="term-status-percent">{progressLabel(t().progress!)}</span>
        </button>
      )}
    </Show>
  );
}
registerStatusItem({ id: "terminal.progress", align: "right", order: -30, component: TerminalProgressStatus });

registerToolWindow({
  id: "terminal",
  title: t("toolwindow.terminal.title"),
  icon: SquareTerminal,
  anchor: "bottom",
  order: 40,
  shortcut: "Alt+F12",
  component: TerminalView,
  headerContent: TerminalTabs,
  badge: TerminalStripeBadge,
});

/** Open a new terminal tab in `dir` (relative to the project root), e.g. "Open In > Terminal". */
export function openTerminalAt(dir?: string): void {
  const blocked = newTabBlocked();
  if (blocked) return void showBalloon(tr("tools.terminal.terminal"), blocked);
  const tab = newTab(dir);
  showToolWindow("terminal");
  requestAnimationFrame(() => focusSession(tab.id));
}

/** Open a terminal tab running a program (Services: exec into a container or pod). */
export function openTerminalWith(command: TerminalProgram & { title: string }): void {
  const tab = newTab(undefined, command);
  showToolWindow("terminal");
  requestAnimationFrame(() => focusSession(tab.id));
}

/** Open a Terminal tab with an SSH session (Tools | Start SSH Session…). */
export function openSshTerminal(title: string, ssh: SshTarget): void {
  const tab = newSshTab(title, ssh);
  showToolWindow("terminal");
  requestAnimationFrame(() => focusSession(tab.id));
}
setSshTabOpener(openSshTerminal);
setSshHostKeyResolver(confirmSshHostKey);

registerKeymapActions([
  {
    id: "RunSshConsole",
    title: "Start SSH Session…",
    group: "Main Menu/Tools",
    shortcuts: { $default: [], macOS: [] },
  },
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
    showToolWindow("terminal");
    const id = activeTab();
    if (id) requestAnimationFrame(() => focusSession(id));
  },
});
registerCommand({ id: "Terminal.NewTab", title: "New Terminal Tab", category: "Terminal", run: () => openTerminalAt() });
registerCommand({ id: "RunSshConsole", title: "Start SSH Session…", category: "Tools", run: () => chooseSshSession() });
registerCommand({
  id: "Terminal.CloseTab",
  title: "Close Terminal Tab",
  category: "Terminal",
  run: () => {
    const id = activeTab();
    if (id) closeTerminalTab(id);
  },
});
