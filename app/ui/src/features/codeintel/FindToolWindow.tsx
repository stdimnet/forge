/**
 * The Find tool window (Alt+3): results pinned by Find Usages (Alt+F7), the Show Usages
 * popup and Find in Files ("Open in Find Window"), one tab per search. Usages are a tree
 * grouped by usage type and file like PHPStorm's; text occurrences are grouped by file.
 */
import { createSignal, For, Show } from "solid-js";
import { ArrowDown, ArrowUp, ChevronDown, ChevronRight, ChevronsDownUp, ChevronsUpDown, ListTree, RotateCw, Search, Settings, X } from "lucide-solid";
import { FileIcon } from "@/shell/FileIcon";
import { showBalloon } from "@/keymap";
import type { UsageGroup } from "@/ipc/codeintel";
import { FindWindow } from "@/features/find/FindWindow";
import { UsageCode } from "./ShowUsagesPopup";
import {
  activeFindTab,
  closeFindTab,
  countLabel,
  findTabs,
  findUsagesSettingsFor,
  gotoFindOccurrence,
  openUsage,
  SEARCH_TAB,
  searchTabOpen,
  selectFindUsage,
  setActiveFindTab,
  tabTree,
  updateFindTab,
  usageKey,
  type FindTab,
} from "./usages";

const fileName = (path: string) => path.split("/").pop() ?? path;
const dirName = (path: string) => path.split("/").slice(0, -1).join("/");

const [groupByType, setGroupByType] = createSignal(true);
export { groupByType as findGroupByType };
/** Collapsed nodes by key (`g:<label>`, `f:<label>:<path>`), per tab. */
const [collapsed, setCollapsed] = createSignal<Record<number, Set<string>>>({});
const [rerunning, setRerunning] = createSignal(false);

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

/** "3 usages" / "3 matches". */
const resultLabel = (tab: FindTab, n: number) => (tab.kind === "text" ? countLabel(n, "match", "matches") : countLabel(n));

/** Text results and ungrouped usages are listed by file. */
const grouped = (tab: FindTab) => tab.kind === "usages" && groupByType();

function setAll(tab: FindTab, collapse: boolean): void {
  const keys = new Set<string>();
  if (collapse) {
    for (const g of tabTree(tab, groupByType()).groups) {
      keys.add(`g:${g.label}`);
      for (const f of g.files) keys.add(`f:${g.label}:${f.path}`);
    }
  }
  setCollapsed((all) => ({ ...all, [tab.id]: keys }));
}

async function rerun(tab: FindTab): Promise<void> {
  if (!tab.rerun || rerunning()) return;
  setRerunning(true);
  try {
    updateFindTab(tab.id, await tab.rerun());
  } catch (e) {
    showBalloon("Search failed", String(e instanceof Error ? e.message : e));
  } finally {
    setRerunning(false);
  }
}

export function FindTabs() {
  return (
    <div class="run-tabs" role="tablist">
      <Show when={searchTabOpen()}>
        <div
          class="run-tab"
          classList={{ active: activeFindTab() === SEARCH_TAB }}
          role="tab"
          onClick={() => setActiveFindTab(SEARCH_TAB)}
          onMouseDown={(e) => e.button === 1 && (e.preventDefault(), closeFindTab(SEARCH_TAB))}
        >
          <span class="run-tab-title">Find in Files</span>
          <button
            class="run-tab-close"
            title="Close Tab"
            onClick={(e) => {
              e.stopPropagation();
              closeFindTab(SEARCH_TAB);
            }}
          >
            <X size={12} />
          </button>
        </div>
      </Show>
      <For each={findTabs()}>
        {(t) => (
          <div
            class="run-tab"
            classList={{ active: activeFindTab() === t.id }}
            role="tab"
            onClick={() => setActiveFindTab(t.id)}
            onMouseDown={(e) => e.button === 1 && (e.preventDefault(), closeFindTab(t.id))}
          >
            <span class="run-tab-title">{t.title}</span>
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
              <span class="find-count">{resultLabel(props.tab, f.usages.length)}</span>
            </div>
            <Show when={!isCollapsed(props.tab.id, fkey())}>
              <For each={f.usages}>
                {(row) => {
                  const item = { path: f.path, row };
                  return (
                    <div
                      class="find-node find-usage"
                      classList={{ nested: props.grouped, selected: props.tab.selected === usageKey(item) }}
                      title={`${f.path}:${row.line + 1}`}
                      onClick={() => {
                        selectFindUsage(props.tab.id, usageKey(item));
                        openUsage(item);
                      }}
                    >
                      <span class="find-line">{row.line + 1}</span>
                      <UsageCode row={row} />
                    </div>
                  );
                }}
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
  // The Find in Files tab is the live search form with Replace (`features/find`).
  return (
    <Show when={!searchTabOpen() || activeFindTab() !== SEARCH_TAB} fallback={<FindWindow />}>
      <ResultTab />
    </Show>
  );
}

/** The active tab of pinned results. */
function ResultTab() {
  const tab = () => findTabs().find((t) => t.id === activeFindTab());
  return (
    <div class="find-window">
      <Show
        when={tab()}
        fallback={
          <div class="find-empty">
            <Search size={28} class="muted" />
            <div>Usages found with Find Usages (Alt+F7) and Find in Files results are shown here.</div>
          </div>
        }
      >
        {(t) => (
          <>
            <div class="find-toolbar" role="toolbar">
              <Show when={t().rerun}>
                <button class="find-tool" title="Rerun" disabled={rerunning()} onClick={() => void rerun(t())}>
                  <RotateCw size={15} />
                </button>
              </Show>
              <Show when={t().kind === "usages"}>
                <button
                  class="find-tool"
                  classList={{ on: groupByType() }}
                  title="Group by Usage Type"
                  aria-pressed={groupByType()}
                  onClick={() => setGroupByType(!groupByType())}
                >
                  <ListTree size={15} />
                </button>
              </Show>
              <button class="find-tool" title="Expand All" onClick={() => setAll(t(), false)}>
                <ChevronsUpDown size={15} />
              </button>
              <button class="find-tool" title="Collapse All" onClick={() => setAll(t(), true)}>
                <ChevronsDownUp size={15} />
              </button>
              <Show when={t().kind === "usages" && t().origin}>
                <button class="find-tool" title="Settings… (Ctrl+Alt+Shift+F7)" aria-label="Find Usages Settings" onClick={() => findUsagesSettingsFor(t())}>
                  <Settings size={15} />
                </button>
              </Show>
              <button class="find-tool" title="Previous Occurrence" onClick={() => gotoFindOccurrence(-1, groupByType())}>
                <ArrowUp size={15} />
              </button>
              <button class="find-tool" title="Next Occurrence" onClick={() => gotoFindOccurrence(1, groupByType())}>
                <ArrowDown size={15} />
              </button>
            </div>
            <div class="find-tree" role="tree">
              <div class="find-node find-root">
                <span>{t().header}</span>
                <span class="find-count">
                  {resultLabel(t(), tabTree(t(), groupByType()).total)} in{" "}
                  {countLabel(tabTree(t(), groupByType()).fileCount, "file")}
                </span>
              </div>
              <For each={tabTree(t(), groupByType()).groups}>{(g) => <Group tab={t()} group={g} grouped={grouped(t())} />}</For>
            </div>
          </>
        )}
      </Show>
    </div>
  );
}
