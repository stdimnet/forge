import { For, Show, createSignal, type JSX } from "solid-js";
import { Dynamic } from "solid-js/web";
import { Pin, X } from "lucide-solid";
import {
  editor,
  activateTab,
  closeTabIn,
  customTabs,
  editorGroups,
  fileSystemFor,
  focusGroup,
  isCustomTab,
  isDirty,
  isTabPinned,
  splitOrientation,
  togglePinTab,
  type EditorTab,
} from "@/state/editor";
import type { TabGroup } from "@/state/tabGroups";
import { statusOf } from "@/state/vcs";
import { getSetting } from "@/state/settings";
import { CodeEditor } from "@/editor/CodeEditor";
import { FileIcon } from "./FileIcon";
import { editorViewFor, fileEditorFor } from "./editorViews";
import { canDragTabsOut, openTabMenu, tabDraggedOut } from "./tabMenu";
import { NavigationBar, navBarLocation } from "@/features/navBar";
import { tabColor } from "@/features/scopes/model";

const docOf = (id: string): EditorTab | undefined => editor.tabs.find((t) => t.path === id);
const customOf = (id: string) => customTabs().find((t) => t.id === id);

function TabIcon(props: { tab: EditorTab }) {
  return (
    <Show
      when={fileSystemFor(props.tab.path)?.icon}
      fallback={
        <Show when={editorViewFor(props.tab)?.icon} fallback={<FileIcon name={props.tab.name} size={14} />}>
          {(Icon) => {
            const I = Icon();
            return <I size={14} tab={props.tab} />;
          }}
        </Show>
      }
    >
      {(Icon) => <Dynamic component={Icon()} size={14} name={props.tab.name} />}
    </Show>
  );
}

/** Pixels the pointer moves before a press on a tab counts as a drag. */
const DRAG_START = 6;

/**
 * Dragging a tab out of the window (releasing the pointer outside it) hands it to
 * `tabDraggedOut` (open it in a new window). Pointer capture keeps the events coming while
 * the pointer is outside the page.
 */
function dragTabOut(e: PointerEvent, group: number, id: string): void {
  if (e.button !== 0 || !canDragTabsOut() || (e.target as Element).closest("button")) return;
  const tab = e.currentTarget as HTMLElement;
  const [x0, y0] = [e.clientX, e.clientY];
  let dragging = false;
  const move = (ev: PointerEvent) => {
    if (dragging || Math.hypot(ev.clientX - x0, ev.clientY - y0) < DRAG_START) return;
    dragging = true;
    tab.setPointerCapture(ev.pointerId);
    tab.classList.add("dragging");
  };
  const up = (ev: PointerEvent) => {
    tab.removeEventListener("pointermove", move);
    tab.removeEventListener("pointerup", up);
    tab.removeEventListener("pointercancel", up);
    tab.classList.remove("dragging");
    if (!dragging || ev.type === "pointercancel") return;
    const outside = ev.clientX < 0 || ev.clientY < 0 || ev.clientX > window.innerWidth || ev.clientY > window.innerHeight;
    if (outside) tabDraggedOut(group, id);
  };
  tab.addEventListener("pointermove", move);
  tab.addEventListener("pointerup", up);
  tab.addEventListener("pointercancel", up);
}

/** One editor tab: file, view or custom tab. Pinned tabs show a pin that unpins on click. */
function Tab(props: { group: TabGroup; id: string }) {
  const active = () => props.group.active === props.id;
  const pinned = () => isTabPinned(props.id);
  const doc = () => docOf(props.id);
  const custom = () => (isCustomTab(props.id) ? customOf(props.id) : undefined);
  const status = () => (doc() ? statusOf(props.id) : undefined);
  const close = (e: MouseEvent) => {
    e.stopPropagation();
    closeTabIn(props.group.id, props.id);
  };
  const label = (): JSX.Element => doc()?.name ?? custom()?.title ?? props.id;
  const color = () => (doc() ? tabColor(props.id) : undefined);
  return (
    <div
      class="editor-tab"
      classList={{ active: active(), pinned: pinned(), "file-colored": !!color() }}
      style={{ "--file-color": color() }}
      role="tab"
      aria-selected={active()}
      title={custom()?.tooltip ?? custom()?.title ?? props.id}
      data-tab-id={props.id}
      onClick={() => activateTab(props.group.id, props.id)}
      onPointerDown={(e) => dragTabOut(e, props.group.id, props.id)}
      onAuxClick={(e) => e.button === 1 && closeTabIn(props.group.id, props.id)}
      onContextMenu={(e) => {
        e.preventDefault();
        activateTab(props.group.id, props.id);
        openTabMenu(e.clientX, e.clientY, props.group.id, props.id);
      }}
    >
      <Show
        when={doc()}
        fallback={
          <Show when={custom()?.icon}>
            {(icon) => (
              <span class="editor-tab-icon">
                <Dynamic component={icon()} size={14} />
              </span>
            )}
          </Show>
        }
      >
        {(d) => <TabIcon tab={d()} />}
      </Show>
      <span class={status() ? `vcs-${status()}` : ""}>{label()}</span>
      <Show
        when={pinned() && getSetting<boolean>("editor.tabs.showPinnedIcon", true) !== false}
        fallback={
          <button class="tab-close" classList={{ dirty: !!doc() && isDirty(doc()!) }} title="Close (Ctrl+F4)" onClick={close}>
            <span class="dirty-dot" />
            <X size={13} />
          </button>
        }
      >
        <button
          class="tab-close tab-pin"
          classList={{ dirty: !!doc() && isDirty(doc()!) }}
          title="Unpin Tab"
          onClick={(e) => {
            e.stopPropagation();
            togglePinTab(props.id);
          }}
        >
          <span class="dirty-dot" />
          <Pin size={12} />
        </button>
      </Show>
    </div>
  );
}

function Tabs(props: { group: TabGroup }) {
  return (
    <div class="editor-tabs" role="tablist">
      <For each={props.group.tabs}>{(id) => <Tab group={props.group} id={id} />}</For>
    </div>
  );
}

function Welcome() {
  const rows: [string, string][] = [
    ["Search Everywhere", "Double Shift"],
    ["Go to File", "Ctrl+Shift+N"],
    ["Recent Files", "Ctrl+E"],
    ["Navigation Bar", "Alt+Home"],
    ["Drop files here to open them", ""],
  ];
  return (
    <div class="editor-empty">
      <img src="/forge.svg" alt="" class="editor-empty-logo" />
      <div class="editor-empty-rows">
        <For each={rows}>
          {([label, key]) => (
            <div class="editor-empty-row">
              <span>{label}</span>
              <span class="kbd-hint">{key}</span>
            </div>
          )}
        </For>
      </div>
    </div>
  );
}

/** The content of one tab inside a group. */
function Slot(props: { group: TabGroup; id: string }) {
  const hidden = () => props.group.active !== props.id;
  return (
    <Show
      when={docOf(props.id)}
      fallback={
        <Show when={customOf(props.id)}>
          {(c) => (
            <div class="editor-slot editor-slot-custom" classList={{ hidden: hidden() }}>
              <Dynamic component={c().component} />
            </div>
          )}
        </Show>
      }
    >
      {(tab) => (
        <div class="editor-slot" classList={{ hidden: hidden() }}>
          <Show
            when={editorViewFor(tab())}
            fallback={
              <Show when={fileEditorFor(tab())} fallback={<CodeEditor tab={tab()} group={props.group.id} />}>
                {(provider) => <Dynamic component={provider().component} tab={tab()} group={props.group.id} />}
              </Show>
            }
          >
            {(view) => {
              const View = view().component;
              return <View tab={tab()} />;
            }}
          </Show>
        </div>
      )}
    </Show>
  );
}

/** One editor group (a split): its own tabs, navigation bar and editors. */
function GroupView(props: { group: TabGroup; weight: number }) {
  const isActive = () => editor.layout.activeGroup === props.group.id;
  return (
    <section
      class="editor-group"
      classList={{ "active-group": isActive() && editorGroups().length > 1 }}
      style={{ "flex-grow": props.weight }}
      data-editor-group={props.group.id}
      onFocusIn={() => focusGroup(props.group.id)}
      onMouseDown={() => focusGroup(props.group.id)}
    >
      <Tabs group={props.group} />
      <Show when={props.group.active && !isCustomTab(props.group.active) && navBarLocation() === "top"}>
        <NavigationBar placement="top" group={props.group.id} tab={() => (props.group.active ? docOf(props.group.active) : undefined)} />
      </Show>
      <div class="editor-stack">
        <For each={props.group.tabs}>{(id) => <Slot group={props.group} id={id} />}</For>
      </div>
    </section>
  );
}

/** Relative sizes of the groups, by group id (missing = 1). */
const [weights, setWeights] = createSignal<Record<number, number>>({});
const weightOf = (id: number) => weights()[id] ?? 1;

/** Drag handle between two editor groups. */
function GroupSplitter(props: { before: TabGroup; after: TabGroup }) {
  const onPointerDown = (e: PointerEvent) => {
    e.preventDefault();
    const container = (e.currentTarget as HTMLElement).parentElement!;
    const vertical = splitOrientation() === "vertical";
    const total = vertical ? container.clientWidth : container.clientHeight;
    const sum = editorGroups().reduce((s, g) => s + weightOf(g.id), 0);
    const a = weightOf(props.before.id);
    const b = weightOf(props.after.id);
    const start = vertical ? e.clientX : e.clientY;
    const move = (ev: PointerEvent) => {
      const delta = (((vertical ? ev.clientX : ev.clientY) - start) / Math.max(1, total)) * sum;
      const min = 0.15 * (a + b);
      const na = Math.min(a + b - min, Math.max(min, a + delta));
      setWeights((w) => ({ ...w, [props.before.id]: na, [props.after.id]: a + b - na }));
    };
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };
  return <div class={`editor-group-splitter ${splitOrientation() === "vertical" ? "v" : "h"}`} onPointerDown={onPointerDown} />;
}

export function EditorArea() {
  const hasTabs = () => editorGroups().some((g) => g.tabs.length > 0);
  return (
    <main class="editor-area">
      <Show when={hasTabs()} fallback={<Welcome />}>
        <div class="editor-groups" classList={{ horizontal: splitOrientation() === "horizontal" }}>
          <For each={editorGroups()}>
            {(group, i) => (
              <>
                <Show when={i() > 0}>
                  <GroupSplitter before={editorGroups()[i() - 1]} after={group} />
                </Show>
                <GroupView group={group} weight={weightOf(group.id)} />
              </>
            )}
          </For>
        </div>
      </Show>
    </main>
  );
}
