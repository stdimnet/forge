/**
 * Terminal tool window: tabs in the header ("Local", "Local (2)", +), one xterm.js per tab,
 * opened in the project root. Tabs keep their shell while hidden.
 */
import { createEffect, For, on, onCleanup, onMount, Show } from "solid-js";
import { Check, ChevronDown, Plus, Users, X } from "lucide-solid";
import { isToolWindowVisible } from "@/state/layout";
import { theme } from "@/state/theme";
import { openMenu, type MenuItem } from "@/features/run/PopupMenu";
import {
  activeTab,
  closeOtherTabs,
  closeTab,
  displayTitle,
  newTab,
  newTabBlocked,
  newTabHere,
  renameTab,
  restoringTabs,
  renaming,
  setActiveTab,
  setRenaming,
  shellProfiles,
  tabById,
  tabIds,
  tabs,
  terminalTabActions,
  type TerminalProgram,
  type TerminalSource,
  type TerminalTab,
} from "./store";
import type { SshTarget } from "@/ipc/terminal";
import { chooseSshSession } from "./ssh";
import {
  applyThemeToAll,
  attachSession,
  clearBuffer,
  copySelection,
  disposeSession,
  fitSession,
  focusSession,
  hasSelection,
  paste,
  selectAll,
  updateSharedSession,
} from "./sessions";
import { progressLabel } from "./osc";
import { ProgressRing } from "./ProgressRing";
import { t } from "@/i18n";
import "@xterm/xterm/css/xterm.css";
import "./terminal.css";

export function closeTerminalTab(id: string): void {
  disposeSession(id);
  closeTab(id);
}

function tabMenu(tab: TerminalTab, x: number, y: number) {
  const blocked = !!newTabBlocked();
  const extra = terminalTabActions(tab);
  const items: MenuItem[] = [
    { label: t("tools.terminal.newTab"), disabled: blocked, run: () => newTab() },
    { label: t("tools.terminal.newTabInCurrentDirectory"), disabled: blocked, run: () => newTabHere() },
    { label: t("tools.terminal.renameSession"), run: () => setRenaming(tab.id) },
    { separator: true },
    ...extra.flatMap((a): MenuItem[] => [{ label: a.label, icon: a.checked ? Check : undefined, run: a.run }]),
    ...(extra.length ? [{ separator: true }] : []),
    { label: t("common.closeTab"), shortcut: "Ctrl+Shift+F4", run: () => closeTerminalTab(tab.id) },
    {
      label: t("tools.terminal.closeOtherTabs"),
      disabled: tabs().length < 2,
      run: () => closeOtherTabs(tab.id).forEach(disposeSession),
    },
  ];
  openMenu({ x, y, items });
}

function tabTooltip(tab: TerminalTab): string {
  const lines = [displayTitle(tab)];
  if (tab.renamed && tab.appTitle) lines.push(tab.appTitle);
  if (tab.liveCwd) lines.push(tab.liveCwd);
  if (tab.progress) lines.push(t("tools.terminal.progressTooltip", { progress: progressLabel(tab.progress) }));
  if (tab.shared) lines.push(t("tools.terminal.sharedWithCodeWithMeGuests"));
  if (tab.source?.kind === "collab") lines.push(tab.source.readOnly ? t("tools.terminal.theHostsTerminalReadOnly") : t("tools.terminal.theHostsTerminal"));
  return lines.join("\n");
}

export function TerminalTabs() {
  return (
    <div class="term-tabs" role="tablist">
      <For each={tabIds()}>
        {(id) => (
          <Show when={tabById(id)}>
            {(tab) => (
              <div
                class="term-tab"
                classList={{ active: activeTab() === tab().id }}
                role="tab"
                aria-selected={activeTab() === tab().id}
                data-terminal-tab={tab().title}
                title={tabTooltip(tab())}
                onMouseDown={(e) => {
                  if (e.button === 1) {
                    e.preventDefault();
                    closeTerminalTab(tab().id);
                  }
                }}
                onClick={() => {
                  setActiveTab(tab().id);
                  queueMicrotask(() => focusSession(tab().id));
                }}
                onDblClick={() => setRenaming(tab().id)}
                onContextMenu={(e) => {
                  e.preventDefault();
                  tabMenu(tab(), e.clientX, e.clientY);
                }}
              >
                <Show when={tab().progress}>{(p) => <ProgressRing progress={p()} />}</Show>
                <Show when={tab().shared || tab().source?.kind === "collab"}>
                  <Users size={12} class="term-tab-shared" aria-label={t("tools.terminal.shared")} />
                </Show>
                <Show when={tab().bell && activeTab() !== tab().id}>
                  <span class="term-tab-bell" title={t("tools.terminal.bell")} />
                </Show>
                <Show when={renaming() === tab().id} fallback={<span class="term-tab-title">{displayTitle(tab())}</span>}>
                  <input
                    class="term-tab-input"
                    value={tab().title}
                    ref={(el) => queueMicrotask(() => (el.focus(), el.select()))}
                    onBlur={(e) => renameTab(tab().id, e.currentTarget.value)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") renameTab(tab().id, e.currentTarget.value);
                      if (e.key === "Escape") setRenaming(null);
                      e.stopPropagation();
                    }}
                  />
                </Show>
                <button
                  class="term-tab-close"
                  title={t("common.closeTab")}
                  onClick={(e) => {
                    e.stopPropagation();
                    closeTerminalTab(tab().id);
                  }}
                >
                  <X size={12} />
                </button>
              </div>
            )}
          </Show>
        )}
      </For>
      <Show when={!newTabBlocked()}>
        <button class="icon-btn term-new" title={t("tools.terminal.newTabCtrlShiftT")} onClick={() => newTab()}>
          <Plus size={15} />
        </button>
      </Show>
      <button
        class="icon-btn term-new"
        title={t("tools.terminal.newTabOptions")}
        onClick={(e) => {
          const r = e.currentTarget.getBoundingClientRect();
          const id = activeTab();
          openMenu({
            x: r.left,
            y: r.bottom + 4,
            items: [
              { label: t("tools.terminal.newTab"), shortcut: "Ctrl+Shift+T", disabled: !!newTabBlocked(), run: () => newTab() },
              ...shellProfiles().map((p) => ({ label: p.label, disabled: !!newTabBlocked(), run: () => void newTab(p.cwd, p.command, p.label) })),
              { label: t("tools.terminal.newTabInCurrentDirectory"), disabled: !id || !!newTabBlocked(), run: () => newTabHere() },
              { label: t("tools.terminal.sshSessionEllipsis"), run: () => void chooseSshSession() },
              { separator: true },
              { label: t("tools.terminal.renameSession"), disabled: !id, run: () => id && setRenaming(id) },
              { label: t("tools.terminal.clearBuffer"), disabled: !id, run: () => id && clearBuffer(id) },
            ],
          });
        }}
      >
        <ChevronDown size={14} />
      </button>
    </div>
  );
}

function TerminalHost(props: { id: string; cwd?: string; command?: TerminalProgram; ssh?: SshTarget; source?: TerminalSource }) {
  let host!: HTMLDivElement;
  const visible = () => activeTab() === props.id && isToolWindowVisible("terminal");
  onMount(() => {
    attachSession(props.id, host, props.cwd, props.command, props.ssh, props.source);
    const ro = new ResizeObserver(() => visible() && fitSession(props.id));
    ro.observe(host);
    onCleanup(() => ro.disconnect());
  });
  createEffect(
    on(visible, (v) => {
      if (v)
        requestAnimationFrame(() => {
          fitSession(props.id);
          focusSession(props.id);
        });
    }),
  );
  // A host's shared terminal: its size and whether this guest may type follow the host.
  createEffect(
    on(
      () => {
        const s = tabById(props.id)?.source;
        return s?.kind === "collab" ? `${s.cols}x${s.rows}:${s.readOnly}` : null;
      },
      () => {
        const s = tabById(props.id)?.source;
        if (s?.kind === "collab") updateSharedSession(props.id, s.cols, s.rows, s.readOnly);
      },
      { defer: true },
    ),
  );
  return (
    <div
      ref={host}
      class="term-host"
      classList={{ hidden: activeTab() !== props.id }}
      onContextMenu={(e) => {
        e.preventDefault();
        const id = props.id;
        openMenu({
          x: e.clientX,
          y: e.clientY,
          items: [
            { label: t("common.copy"), shortcut: "Ctrl+Shift+C", disabled: !hasSelection(id), run: () => void copySelection(id) },
            { label: t("tools.terminal.paste"), shortcut: "Ctrl+Shift+V", run: () => void paste(id) },
            { label: t("tools.terminal.selectAll"), run: () => selectAll(id) },
            { separator: true },
            { label: t("tools.terminal.clearBuffer"), run: () => clearBuffer(id) },
            { separator: true },
            { label: t("tools.terminal.newTab"), shortcut: "Ctrl+Shift+T", disabled: !!newTabBlocked(), run: () => newTab() },
            { label: t("tools.terminal.newTabInCurrentDirectory"), disabled: !!newTabBlocked(), run: () => newTabHere() },
            { label: t("tools.terminal.renameSession"), run: () => setRenaming(id) },
            { label: t("common.closeTab"), run: () => closeTerminalTab(id) },
          ],
        });
      }}
    />
  );
}

export function TerminalView() {
  // Opening the tool window without a session starts one, like PHPStorm.
  createEffect(() => {
    if (isToolWindowVisible("terminal") && tabs().length === 0 && !newTabBlocked() && !restoringTabs()) newTab();
  });
  createEffect(on(theme, () => requestAnimationFrame(applyThemeToAll), { defer: true }));
  return (
    <div class="terminal-view">
      <For each={tabIds()}>
        {(id) => <TerminalHost id={id} cwd={tabById(id)?.cwd} command={tabById(id)?.command} ssh={tabById(id)?.ssh} source={tabById(id)?.source} />}
      </For>
      <Show when={tabs().length === 0}>
        <div class="placeholder-panel">
          <Show
            when={!newTabBlocked()}
            fallback={newTabBlocked()}
          >
            {t("tools.terminal.noTerminalSessions")}
            <button class="link-btn" onClick={() => newTab()}>
              {t("tools.terminal.openANewTab")}
            </button>
          </Show>
        </div>
      </Show>
    </div>
  );
}
