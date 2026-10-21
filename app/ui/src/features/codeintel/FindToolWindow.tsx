/**
 * The Find tool window (Alt+3): usages pinned by Find Usages (Alt+F7) or the Show Usages
 * popup, one tab per search, as a tree grouped by usage type and file like PHPStorm's.
 */
import { createSignal, For, Show } from "solid-js";
import { ChevronDown, ChevronRight, ListTree, Search, X, ChevronsDownUp, ChevronsUpDown } from "lucide-solid";
import { FileIcon } from "@/shell/FileIcon";
import type { UsageGroup, UsageTree } from "@/ipc/codeintel";
import { UsageCode } from "./ShowUsagesPopup";
import { activeFindTab, closeFindTab, countLabel, findTabs, openUsage, setActiveFindTab, type FindTab } from "./usages";

const fileName = (path: string) => path.split("/").pop() ?? path;
const dirName = (path: string) => path.split("/").slice(0, -1).join("/");

const [groupByType, setGroupByType] = createSignal(true);
/** Collapsed nodes by key (`g:<label>`, `f:<label>:<path>`), per tab. */
const [collapsed, setCollapsed] = createSignal<Record<number, Set<string>>>({});

function isCollapsed(tab: number, key: string): boolean {
  return collapsed()[tab]?.has(key) ?? false;
}

function toggle(tab: number, key: string): void {
  setCollapsed((all) => {
    const set = new Set(all[tab] ?? []);
    if (set.has(key)) set.delete(key);
    else set.add(key);
    return { ...all, [tab]: set };
  });
}

function setAll(tab: FindTab, collapse: boolean): void {
  const keys = new Set<string>();
  if (collapse) {
    for (const g of tree(tab).groups) {
      keys.add(`g:${g.label}`);
      for (const f of g.files) keys.add(`f:${g.label}:${f.path}`);
    }
  }
  setCollapsed((all) => ({ ...all, [tab.id]: keys }));
}

const tree = (tab: FindTab): UsageTree => (groupByType() ? tab.answer.byType : tab.answer.byFile);

export function FindTabs() {
  return (
    <div class="run-tabs" role="tablist">
      <For each={findTabs()}>
        {(t) => (
          <div
            class="run-tab"
            classList={{ active: activeFindTab() === t.id }}
            role="tab"
            onClick={() => setActiveFindTab(t.id)}
            onMouseDown={(e) => e.button === 1 && (e.preventDefault(), closeFindTab(t.id))}
          >
            <span class="run-tab-title">Usages of {t.answer.title}</span>
            <button
              class="run-tab-close"
              title="Close Tab"
              onClick={(e) => {
                e.stopPropagation();
                closeFindTab(t.id);
              }}
            >
              <X size={12} />
            </button>
          </div>
        )}
      </For>
    </div>
  );
}

function Group(props: { tab: FindTab; group: UsageGroup; grouped: boolean }) {
  const key = () => `g:${props.group.label}`;
  const files = () => (
    <For each={props.group.files}>
      {(f) => {
        const fkey = () => `f:${props.group.label}:${f.path}`;
        return (
          <>
            <div
              class="find-node find-file"
              classList={{ nested: props.grouped }}
              onClick={() => toggle(props.tab.id, fkey())}
            >
              <Show when={isCollapsed(props.tab.id, fkey())} fallback={<ChevronDown size={13} class="muted" />}>
                <ChevronRight size={13} class="muted" />
              </Show>
              <FileIcon name={fileName(f.path)} size={14} />
              <span class="find-file-name">{fileName(f.path)}</span>
              <span class="find-dir">{dirName(f.path)}</span>
              <span class="find-count">{countLabel(f.usages.length)}</span>
            </div>
            <Show when={!isCollapsed(props.tab.id, fkey())}>
              <For each={f.usages}>
                {(row) => (
                  <div
                    class="find-node find-usage"
                    classList={{ nested: props.grouped }}
                    title={`${f.path}:${row.line + 1}`}
                    onClick={() => openUsage({ path: f.path, row })}
                  >
                    <span class="find-line">{row.line + 1}</span>
                    <UsageCode row={row} />
                  </div>
                )}
              </For>
            </Show>
          </>
        );
      }}
    </For>
  );
  return (
    <Show when={props.grouped} fallback={files()}>
      <div class="find-node find-group" onClick={() => toggle(props.tab.id, key())}>
        <Show when={isCollapsed(props.tab.id, key())} fallback={<ChevronDown size={13} class="muted" />}>
          <ChevronRight size={13} class="muted" />
        </Show>
        <span class="find-group-label">{props.group.label}</span>
        <span class="find-count">{countLabel(props.group.count)}</span>
      </div>
      <Show when={!isCollapsed(props.tab.id, key())}>{files()}</Show>
    </Show>
  );
}

export function FindToolWindow() {
  const tab = () => findTabs().find((t) => t.id === activeFindTab());
  return (
    <div class="find-window">
      <Show
        when={tab()}
        fallback={
          <div class="find-empty">
            <Search size={28} class="muted" />
            <div>Usages found with Find Usages (Alt+F7) are shown here.</div>
          </div>
        }
      >
        {(t) => (
          <>
            <div class="find-toolbar" role="toolbar">
              <button
                class="find-tool"
                classList={{ on: groupByType() }}
                title="Group by Usage Type"
                aria-pressed={groupByType()}
                onClick={() => setGroupByType(!groupByType())}
              >
                <ListTree size={15} />
              </button>
              <button class="find-tool" title="Expand All" onClick={() => setAll(t(), false)}>
                <ChevronsUpDown size={15} />
              </button>
              <button class="find-tool" title="Collapse All" onClick={() => setAll(t(), true)}>
                <ChevronsDownUp size={15} />
              </button>
            </div>
            <div class="find-tree" role="tree">
              <div class="find-node find-root">
                <span>
                  Usages of <strong>{t().answer.title}</strong> in Project Files
                </span>
                <span class="find-count">
                  {countLabel(tree(t()).total)} in {countLabel(tree(t()).fileCount, "file")}
                </span>
              </div>
              <For each={tree(t()).groups}>{(g) => <Group tab={t()} group={g} grouped={groupByType()} />}</For>
            </div>
          </>
        )}
      </Show>
    </div>
  );
}
