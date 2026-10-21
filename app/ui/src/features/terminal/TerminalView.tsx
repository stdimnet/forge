/**
 * Terminal tool window: tabs in the header ("Local", "Local (2)", +), one xterm.js per tab,
 * opened in the project root. Tabs keep their shell while hidden.
 */
import { createEffect, For, on, onCleanup, onMount, Show } from "solid-js";
import { ChevronDown, Plus, X } from "lucide-solid";
import { layout } from "@/state/layout";
import { theme } from "@/state/theme";
import { openMenu, type MenuItem } from "@/features/run/PopupMenu";
import {
  activeTab,
  closeOtherTabs,
  closeTab,
  newTab,
  renameTab,
  renaming,
  setActiveTab,
  setRenaming,
  tabs,
  type TerminalTab,
} from "./store";
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
} from "./sessions";
import "@xterm/xterm/css/xterm.css";
import "./terminal.css";

export function closeTerminalTab(id: string): void {
  disposeSession(id);
  closeTab(id);
}

function tabMenu(tab: TerminalTab, x: number, y: number) {
  const items: MenuItem[] = [
    { label: "New Tab", run: () => newTab() },
    { label: "Rename Session", run: () => setRenaming(tab.id) },
    { separator: true },
    { label: "Close Tab", shortcut: "Ctrl+Shift+F4", run: () => closeTerminalTab(tab.id) },
    {
      label: "Close Other Tabs",
      disabled: tabs().length < 2,
      run: () => closeOtherTabs(tab.id).forEach(disposeSession),
    },
  ];
  openMenu({ x, y, items });
}

export function TerminalTabs() {
  return (
    <div class="term-tabs" role="tablist">
      <For each={tabs()}>
        {(tab) => (
          <div
            class="term-tab"
            classList={{ active: activeTab() === tab.id }}
            role="tab"
            aria-selected={activeTab() === tab.id}
            data-terminal-tab={tab.title}
            title={tab.workingDir}
            onMouseDown={(e) => {
              if (e.button === 1) {
                e.preventDefault();
                closeTerminalTab(tab.id);
              }
            }}
            onClick={() => {
              setActiveTab(tab.id);
              queueMicrotask(() => focusSession(tab.id));
            }}
            onDblClick={() => setRenaming(tab.id)}
            onContextMenu={(e) => {
              e.preventDefault();
              tabMenu(tab, e.clientX, e.clientY);
            }}
          >
            <Show when={renaming() === tab.id} fallback={<span class="term-tab-title">{tab.title}</span>}>
              <input
                class="term-tab-input"
                value={tab.title}
                ref={(el) => queueMicrotask(() => (el.focus(), el.select()))}
                onBlur={(e) => renameTab(tab.id, e.currentTarget.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") renameTab(tab.id, e.currentTarget.value);
                  if (e.key === "Escape") setRenaming(null);
                  e.stopPropagation();
                }}
              />
            </Show>
            <button
              class="term-tab-close"
              title="Close Tab"
              onClick={(e) => {
                e.stopPropagation();
                closeTerminalTab(tab.id);
              }}
            >
              <X size={12} />
            </button>
          </div>
        )}
      </For>
      <button class="icon-btn term-new" title="New Tab (Ctrl+Shift+T)" onClick={() => newTab()}>
        <Plus size={15} />
      </button>
      <button
        class="icon-btn term-new"
        title="Options"
        onClick={(e) => {
          const r = e.currentTarget.getBoundingClientRect();
          const id = activeTab();
          openMenu({
            x: r.left,
            y: r.bottom + 4,
            items: [
              { label: "New Tab", shortcut: "Ctrl+Shift+T", run: () => newTab() },
              { label: "Rename Session", disabled: !id, run: () => id && setRenaming(id) },
              { label: "Clear Buffer", disabled: !id, run: () => id && clearBuffer(id) },
            ],
          });
        }}
      >
        <ChevronDown size={14} />
      </button>
    </div>
  );
}

function TerminalHost(props: { tab: TerminalTab }) {
  let host!: HTMLDivElement;
  const visible = () => activeTab() === props.tab.id && layout.active.bottom === "terminal";
  onMount(() => {
    attachSession(props.tab.id, host, props.tab.cwd, props.tab.command);
    const ro = new ResizeObserver(() => visible() && fitSession(props.tab.id));
    ro.observe(host);
    onCleanup(() => ro.disconnect());
  });
  createEffect(
    on(visible, (v) => {
      if (v)
        requestAnimationFrame(() => {
          fitSession(props.tab.id);
          focusSession(props.tab.id);
        });
    }),
  );
  return (
    <div
      ref={host}
      class="term-host"
      classList={{ hidden: activeTab() !== props.tab.id }}
      onContextMenu={(e) => {
        e.preventDefault();
        const id = props.tab.id;
        openMenu({
          x: e.clientX,
          y: e.clientY,
          items: [
            { label: "Copy", shortcut: "Ctrl+Shift+C", disabled: !hasSelection(id), run: () => void copySelection(id) },
            { label: "Paste", shortcut: "Ctrl+Shift+V", run: () => void paste(id) },
            { label: "Select All", run: () => selectAll(id) },
            { separator: true },
            { label: "Clear Buffer", run: () => clearBuffer(id) },
            { separator: true },
            { label: "New Tab", shortcut: "Ctrl+Shift+T", run: () => newTab() },
            { label: "Rename Session", run: () => setRenaming(id) },
            { label: "Close Tab", run: () => closeTerminalTab(id) },
          ],
        });
      }}
    />
  );
}

export function TerminalView() {
  // Opening the tool window without a session starts one, like PHPStorm.
  createEffect(() => {
    if (layout.active.bottom === "terminal" && tabs().length === 0) newTab();
  });
  createEffect(on(theme, () => requestAnimationFrame(applyThemeToAll), { defer: true }));
  return (
    <div class="terminal-view">
      <For each={tabs()}>{(tab) => <TerminalHost tab={tab} />}</For>
      <Show when={tabs().length === 0}>
        <div class="placeholder-panel">
          No terminal sessions.{" "}
          <button class="link-btn" onClick={() => newTab()}>
            Open a new tab
          </button>
        </div>
      </Show>
    </div>
  );
}
