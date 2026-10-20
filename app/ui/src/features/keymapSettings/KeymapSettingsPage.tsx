/**
 * Settings | Keymap — PHPStorm's keymap page: keymap selector, searchable action tree with
 * shortcuts, "Find Actions by Shortcut", and add/remove shortcuts with conflict warnings.
 */
import { For, Show, createMemo, createSignal, onCleanup, onMount } from "solid-js";
import { ChevronDown, ChevronRight, ChevronsDownUp, ChevronsUpDown, Folder, Keyboard, Pencil, RotateCcw, Search, X } from "lucide-solid";
import {
  ACTION_GROUP_ORDER,
  KEYMAPS,
  actionById,
  addActionShortcut,
  allActions,
  bindings,
  canonicalShortcut,
  findConflicts,
  formatForKeymap,
  isModified,
  keymapId,
  keymapPlatform,
  overrides,
  removeActionShortcut,
  resetAction,
  resetKeymap,
  setActionShortcuts,
  setKeymapId,
  shortcutsOf,
  tryParseShortcut,
  type ActionDef,
  type KeymapId,
} from "@/keymap";
import { ShortcutField } from "./ShortcutField";

const MAIN_MENU_ORDER = ["File", "Edit", "View", "Navigate", "Code", "Refactor", "Build", "Run", "Git", "Window", "Help"];

interface GroupNode {
  path: string;
  name: string;
  children: GroupNode[];
  actions: ActionDef[];
}

function buildTree(actions: ActionDef[]): GroupNode[] {
  const root: GroupNode = { path: "", name: "", children: [], actions: [] };
  for (const a of actions) {
    let node = root;
    for (const part of a.group.split("/")) {
      const path = node.path ? `${node.path}/${part}` : part;
      let child = node.children.find((c) => c.name === part);
      if (!child) {
        child = { path, name: part, children: [], actions: [] };
        node.children.push(child);
      }
      node = child;
    }
    node.actions.push(a);
  }
  const rank = (n: GroupNode) => {
    const top = ACTION_GROUP_ORDER.indexOf(n.path);
    if (top >= 0) return top;
    const menu = n.path.startsWith("Main Menu/") && n.path.split("/").length === 2 ? MAIN_MENU_ORDER.indexOf(n.name) : -1;
    return menu >= 0 ? menu : 100;
  };
  const sort = (n: GroupNode) => {
    n.children.sort((a, b) => rank(a) - rank(b));
    n.children.forEach(sort);
  };
  sort(root);
  return root.children;
}

type Row = { type: "group"; node: GroupNode; depth: number } | { type: "action"; action: ActionDef; depth: number };

/** Dialog to record a one- or two-stroke shortcut, with conflict warnings. */
function ShortcutDialog(props: { action: ActionDef; onClose: () => void }) {
  const [first, setFirst] = createSignal("");
  const [useSecond, setUseSecond] = createSignal(false);
  const [second, setSecond] = createSignal("");
  const text = () => (first() ? (useSecond() && second() ? `${first()} ${second()}` : first()) : "");
  const parsed = () => (text() ? tryParseShortcut(text(), keymapPlatform()) : null);
  const conflicts = createMemo(() => {
    const p = parsed();
    return p ? findConflicts(bindings(), p, props.action.id) : [];
  });

  const apply = (removeOthers: boolean) => {
    if (!parsed()) return;
    if (removeOthers) {
      const canon = canonicalShortcut(text(), keymapPlatform());
      for (const c of conflicts()) {
        if (c.kind !== "same") continue;
        setActionShortcuts(
          c.actionId,
          shortcutsOf(c.actionId).filter((s) => canonicalShortcut(s, keymapPlatform()) !== canon),
        );
      }
    }
    addActionShortcut(props.action.id, text());
    props.onClose();
  };

  return (
    <div class="popup-backdrop modal" onMouseDown={(e) => e.target === e.currentTarget && props.onClose()}>
      <div class="dialog shortcut-dialog" role="dialog" aria-label="Keyboard Shortcut" onKeyDown={(e) => e.key === "Escape" && props.onClose()}>
        <div class="dialog-title">Keyboard Shortcut</div>
        <div class="dialog-body">
          <div class="muted shortcut-dialog-action">{props.action.title.replace(/…$/, "")}</div>
          <div class="form-row">
            <span class="form-label">First stroke:</span>
            <ShortcutField value={first()} onChange={setFirst} autofocus />
          </div>
          <div class="form-row">
            <label class="form-label check">
              <input type="checkbox" checked={useSecond()} onChange={(e) => setUseSecond(e.currentTarget.checked)} />
              Second stroke:
            </label>
            <ShortcutField value={second()} onChange={setSecond} disabled={!useSecond()} />
          </div>
          <div class="shortcut-conflicts">
            <Show when={parsed()} fallback={<span class="muted">Press the keys to use as the shortcut.</span>}>
              <Show when={conflicts().length} fallback={<span class="muted">No conflicts</span>}>
                <div class="conflict-title">
                  {conflicts().some((c) => c.kind === "same")
                    ? "The shortcut is already assigned to other actions:"
                    : "The first stroke starts a shortcut of other actions:"}
                </div>
                <For each={conflicts()}>
                  {(c) => {
                    const a = actionById(c.actionId);
                    return (
                      <div class="conflict-row">
                        <span>{a?.title.replace(/…$/, "") ?? c.actionId}</span>
                        <span class="muted">{a?.group.split("/").join(" | ")}</span>
                        <span class="kbd">{formatForKeymap(shortcutsOf(c.actionId).find((s) => tryParseShortcut(s, keymapPlatform())) ?? "")}</span>
                      </div>
                    );
                  }}
                </For>
              </Show>
            </Show>
          </div>
        </div>
        <div class="dialog-buttons">
          <button class="btn-secondary" onClick={props.onClose}>
            Cancel
          </button>
          <Show
            when={conflicts().some((c) => c.kind === "same")}
            fallback={
              <button class="btn-primary" disabled={!parsed()} onClick={() => apply(false)}>
                OK
              </button>
            }
          >
            <button class="btn-secondary" onClick={() => apply(false)}>
              Leave
            </button>
            <button class="btn-primary" onClick={() => apply(true)}>
              Remove Others
            </button>
          </Show>
        </div>
      </div>
    </div>
  );
}

function ContextMenu(props: { action: ActionDef; x: number; y: number; onClose: () => void; onAdd: () => void }) {
  const onDoc = (e: MouseEvent) => {
    if (!(e.target as Element).closest(".context-menu")) props.onClose();
  };
  onMount(() => document.addEventListener("mousedown", onDoc, true));
  onCleanup(() => document.removeEventListener("mousedown", onDoc, true));
  const item = (label: string, run: () => void, disabled = false) => (
    <button
      class="context-item"
      disabled={disabled}
      onClick={() => {
        run();
        props.onClose();
      }}
    >
      {label}
    </button>
  );
  return (
    <div class="context-menu popup" style={{ left: `${props.x}px`, top: `${props.y}px` }}>
      {item("Add Keyboard Shortcut", props.onAdd)}
      {item("Add Mouse Shortcut", () => {}, true)}
      <Show when={shortcutsOf(props.action.id).length}>
        <div class="context-sep" />
        <For each={shortcutsOf(props.action.id)}>
          {(s) => item(`Remove ${formatForKeymap(s)}`, () => removeActionShortcut(props.action.id, s))}
        </For>
      </Show>
      <div class="context-sep" />
      {item("Reset Shortcuts", () => resetAction(props.action.id), !isModified(props.action.id))}
    </div>
  );
}

export function KeymapSettingsPage() {
  let treeEl!: HTMLDivElement;
  const [filter, setFilter] = createSignal("");
  const [shortcutFilter, setShortcutFilter] = createSignal("");
  const [findOpen, setFindOpen] = createSignal(false);
  const [expanded, setExpanded] = createSignal<Set<string>>(new Set(["Main Menu"]));
  const [selected, setSelected] = createSignal<string | null>(null);
  const [menu, setMenu] = createSignal<{ action: ActionDef; x: number; y: number } | null>(null);
  const [dialog, setDialog] = createSignal<ActionDef | null>(null);

  const visibleActions = createMemo(() => {
    const q = filter().trim().toLowerCase();
    const sc = shortcutFilter() ? canonicalShortcut(shortcutFilter(), keymapPlatform()) : null;
    return allActions().filter((a) => {
      if (q && !a.title.toLowerCase().includes(q) && !a.id.toLowerCase().includes(q)) return false;
      if (sc) {
        // A single-stroke filter also finds chords starting with that stroke.
        return shortcutsOf(a.id).some((s) => {
          const c = canonicalShortcut(s, keymapPlatform());
          return c === sc || c?.startsWith(`${sc} `);
        });
      }
      return true;
    });
  });
  const filtering = () => !!filter().trim() || !!shortcutFilter();
  const tree = createMemo(() => buildTree(visibleActions()));

  const rows = createMemo<Row[]>(() => {
    const out: Row[] = [];
    const walk = (nodes: GroupNode[], depth: number) => {
      for (const node of nodes) {
        out.push({ type: "group", node, depth });
        if (!filtering() && !expanded().has(node.path)) continue;
        walk(node.children, depth + 1);
        for (const action of node.actions) out.push({ type: "action", action, depth: depth + 1 });
      }
    };
    walk(tree(), 0);
    return out;
  });

  const toggle = (path: string) =>
    setExpanded((s) => {
      const n = new Set(s);
      if (n.has(path)) n.delete(path);
      else n.add(path);
      return n;
    });

  const allGroups = () => {
    const paths = new Set<string>();
    const walk = (nodes: GroupNode[]) => nodes.forEach((n) => (paths.add(n.path), walk(n.children)));
    walk(buildTree(allActions()));
    return paths;
  };

  const openMenu = (action: ActionDef, el: HTMLElement) => {
    const r = el.getBoundingClientRect();
    setMenu({ action, x: Math.min(r.left + 160, window.innerWidth - 240), y: r.bottom });
  };

  const onTreeKey = (e: KeyboardEvent) => {
    const list = rows();
    const idx = list.findIndex((r) => (r.type === "action" ? r.action.id : `g:${r.node.path}`) === selected());
    const key = (r: Row) => (r.type === "action" ? r.action.id : `g:${r.node.path}`);
    if (e.key === "ArrowDown" && idx < list.length - 1) setSelected(key(list[idx + 1]));
    else if (e.key === "ArrowUp" && idx > 0) setSelected(key(list[idx - 1]));
    else if ((e.key === "ArrowRight" || e.key === "ArrowLeft") && list[idx]?.type === "group") {
      const path = (list[idx] as { node: GroupNode }).node.path;
      if (expanded().has(path) !== (e.key === "ArrowRight")) toggle(path);
    } else if (e.key === "Enter" && list[idx]?.type === "action") {
      const el = treeEl.querySelector<HTMLElement>(`[data-action="${CSS.escape((list[idx] as { action: ActionDef }).action.id)}"]`);
      if (el) openMenu((list[idx] as { action: ActionDef }).action, el);
    } else return;
    e.preventDefault();
    queueMicrotask(() => treeEl.querySelector(".km-row.selected")?.scrollIntoView({ block: "nearest" }));
  };

  const modifiedCount = () => Object.keys(overrides()[keymapId()] ?? {}).length;

  return (
    <div class="keymap-page">
      <div class="km-top">
        <span class="form-label">Keymap:</span>
        <div class="select-wrap">
          <select value={keymapId()} onChange={(e) => setKeymapId(e.currentTarget.value as KeymapId)}>
            <For each={KEYMAPS}>{(k) => <option value={k.id}>{k.title}</option>}</For>
          </select>
          <ChevronDown size={14} class="select-chevron" />
        </div>
        <Show when={modifiedCount()}>
          <span class="muted km-modified">{modifiedCount()} modified</span>
          <button class="link-btn" onClick={resetKeymap}>
            <RotateCcw size={12} /> Reset
          </button>
        </Show>
      </div>

      <div class="km-toolbar">
        <button class="icon-btn" title="Expand All" onClick={() => setExpanded(allGroups())}>
          <ChevronsUpDown size={15} />
        </button>
        <button class="icon-btn" title="Collapse All" onClick={() => setExpanded(new Set<string>())}>
          <ChevronsDownUp size={15} />
        </button>
        <button
          class="icon-btn"
          title="Edit Shortcuts"
          disabled={!selected() || selected()!.startsWith("g:")}
          onClick={() => {
            const a = actionById(selected() ?? "");
            const el = a && treeEl.querySelector<HTMLElement>(`[data-action="${CSS.escape(a.id)}"]`);
            if (a && el) openMenu(a, el);
          }}
        >
          <Pencil size={14} />
        </button>
        <div class="km-spacer" />
        <div class="search-field km-search">
          <Search size={14} class="muted" />
          <input placeholder="Search actions" value={filter()} onInput={(e) => setFilter(e.currentTarget.value)} />
          <Show when={filter()}>
            <button class="icon-btn" title="Clear" onClick={() => setFilter("")}>
              <X size={12} />
            </button>
          </Show>
        </div>
        <div class="km-find-wrap">
          <button
            class="icon-btn"
            classList={{ active: findOpen() || !!shortcutFilter() }}
            title="Find Actions by Shortcut"
            onClick={() => setFindOpen((v) => !v)}
          >
            <Keyboard size={16} />
          </button>
          <Show when={findOpen()}>
            <div class="popup km-find-popup">
              <div class="km-find-title">Find Actions by Shortcut</div>
              <ShortcutField
                value={shortcutFilter()}
                onChange={(s) => setShortcutFilter(s)}
                placeholder="Press a shortcut…"
                autofocus
              />
              <div class="km-find-hint muted">The list shows actions bound to the pressed keys.</div>
            </div>
          </Show>
        </div>
      </div>

      <Show when={shortcutFilter()}>
        <div class="km-filter-chip">
          <span class="muted">Shortcut:</span>
          <span class="kbd">{formatForKeymap(shortcutFilter())}</span>
          <button class="icon-btn" title="Clear shortcut filter" onClick={() => setShortcutFilter("")}>
            <X size={12} />
          </button>
        </div>
      </Show>

      <div class="km-tree" tabIndex={0} ref={treeEl} onKeyDown={onTreeKey}>
        <Show when={rows().length} fallback={<div class="se-empty">Nothing to show</div>}>
          <For each={rows()}>
            {(row) => {
              if (row.type === "group") {
                const open = () => filtering() || expanded().has(row.node.path);
                return (
                  <div
                    class="km-row km-group"
                    classList={{ selected: selected() === `g:${row.node.path}` }}
                    style={{ "padding-left": `${8 + row.depth * 18}px` }}
                    onClick={() => {
                      setSelected(`g:${row.node.path}`);
                      toggle(row.node.path);
                    }}
                  >
                    {open() ? <ChevronDown size={14} class="muted" /> : <ChevronRight size={14} class="muted" />}
                    <Folder size={14} class="km-folder" />
                    <span>{row.node.name}</span>
                  </div>
                );
              }
              const a = row.action;
              const list = () => shortcutsOf(a.id);
              return (
                <div
                  class="km-row km-action"
                  classList={{ selected: selected() === a.id }}
                  data-action={a.id}
                  style={{ "padding-left": `${8 + row.depth * 18 + 18}px` }}
                  onClick={() => setSelected(a.id)}
                  onDblClick={(e) => openMenu(a, e.currentTarget)}
                  onContextMenu={(e) => {
                    e.preventDefault();
                    setSelected(a.id);
                    setMenu({ action: a, x: e.clientX, y: e.clientY });
                  }}
                >
                  <span class="km-title" classList={{ modified: isModified(a.id) }}>
                    {a.title}
                  </span>
                  <span class="km-shortcuts">
                    <For each={list()}>{(s) => <span class="km-shortcut">{formatForKeymap(s)}</span>}</For>
                  </span>
                </div>
              );
            }}
          </For>
        </Show>
      </div>

      <div class="km-note muted">
        Shortcuts are saved on this device as you change them. Double-click an action or press Enter to edit it.
      </div>

      <Show when={menu()}>
        {(m) => <ContextMenu {...m()} onClose={() => setMenu(null)} onAdd={() => setDialog(m().action)} />}
      </Show>
      <Show when={dialog()}>{(a) => <ShortcutDialog action={a()} onClose={() => setDialog(null)} />}</Show>
    </div>
  );
}
