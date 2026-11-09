/**
 * Settings | Keymap — PHPStorm's keymap page: keymap selector (PHPStorm, VS Code, Eclipse,
 * Sublime Text, Emacs and user keymaps) with Duplicate / Derive / Rename / Delete, searchable
 * action tree with shortcuts, "Find Actions by Shortcut", and adding keyboard and mouse
 * shortcuts with conflict warnings.
 */
import { For, Show, createEffect, createMemo, createSignal, onCleanup, onMount, type JSX } from "solid-js";
import {
  ChevronDown,
  ChevronRight,
  ChevronsDownUp,
  ChevronsUpDown,
  Folder,
  Keyboard,
  Mouse,
  Pencil,
  RotateCcw,
  Search,
  Settings,
  X,
} from "lucide-solid";
import {
  actionById,
  actionGroupOrder,
  activeKeymap,
  addActionShortcut,
  allActions,
  checkShortcut,
  deleteKeymap,
  deriveKeymap,
  displayShortcutsOf,
  duplicateKeymap,
  findActionsByShortcut,
  formatForKeymap,
  isModified,
  keymapId,
  keymapNameProblem,
  keymaps,
  modifiedCount,
  mouseInputOf,
  mouseStrokeOf,
  removeActionShortcut,
  renameKeymap,
  resetAction,
  resetKeymap,
  setKeymapId,
  shortcutsOf,
  suspendKeymap,
  SIDE_BUTTON_EVENT,
  type ActionDef,
  type SideButtonClick,
} from "@/keymap";
import { listen } from "@/ipc";
import type { ShortcutCheck } from "@/ipc/keymap";
import { showContextMenu, item, separator, type MenuEntry } from "@/features/contextMenu/state";
import { askConfirm, askName } from "@/features/contextMenu/dialogs";
import { ShortcutField } from "./ShortcutField";
import { t as tr } from "@/i18n";

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
    const top = actionGroupOrder().indexOf(n.path);
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

/** Validity and conflicts of the shortcut being recorded (from the keymap engine). */
function useShortcutCheck(text: () => string, actionId: string) {
  const [check, setCheck] = createSignal<ShortcutCheck | null>(null);
  createEffect(() => {
    const t = text();
    if (!t) return setCheck(null);
    void checkShortcut(t, actionId).then((c) => text() === t && setCheck(c));
  });
  const parsed = () => (text() && check()?.valid ? check() : null);
  const conflicts = () => parsed()?.conflicts ?? [];
  return { check, parsed, conflicts };
}

/** Conflict list and OK / Leave / Remove Others buttons of the shortcut dialogs. */
function ShortcutDialogFrame(props: {
  title: string;
  action: ActionDef;
  text: string;
  hint: string;
  onClose: () => void;
  children: JSX.Element;
}) {
  const { check, parsed, conflicts } = useShortcutCheck(() => props.text, props.action.id);
  const apply = (removeOthers: boolean) => {
    if (!parsed()) return;
    void addActionShortcut(props.action.id, props.text, removeOthers);
    props.onClose();
  };
  return (
    <div class="popup-backdrop modal" onMouseDown={(e) => e.target === e.currentTarget && props.onClose()}>
      <div class="dialog shortcut-dialog" role="dialog" aria-label={props.title} onKeyDown={(e) => e.key === "Escape" && props.onClose()}>
        <div class="dialog-title">{props.title}</div>
        <div class="dialog-body">
          <div class="muted shortcut-dialog-action">{props.action.title.replace(/…$/, "")}</div>
          {props.children}
          <div class="shortcut-conflicts">
            <Show
              when={parsed()}
              fallback={
                <span class="muted" classList={{ "shortcut-invalid": !!props.text && check()?.valid === false }}>
                  {props.text && check()?.valid === false ? tr("tools.keymapSettings.thisShortcutCannotBeUsed") : props.hint}
                </span>
              }
            >
              <Show when={conflicts().length} fallback={<span class="muted">{tr("tools.keymapSettings.noConflicts")}</span>}>
                <div class="conflict-title">
                  {conflicts().some((c) => c.kind === "same")
                    ? "The shortcut is already assigned to other actions:"
                    : "The first stroke starts a shortcut of other actions:"}
                </div>
                <For each={conflicts()}>
                  {(c) => (
                    <div class="conflict-row">
                      <span>{c.title.replace(/…$/, "")}</span>
                      <span class="muted">{c.group.split("/").join(" | ")}</span>
                      <span class="kbd">{c.display}</span>
                    </div>
                  )}
                </For>
              </Show>
            </Show>
          </div>
        </div>
        <div class="dialog-buttons">
          <button class="btn-secondary" onClick={props.onClose}>
            {tr("common.cancel")}
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
              {tr("tools.keymapSettings.leave")}
            </button>
            <button class="btn-primary" onClick={() => apply(true)}>
              {tr("tools.keymapSettings.removeOthers")}
            </button>
          </Show>
        </div>
      </div>
    </div>
  );
}

/** Dialog to record a one- or two-stroke shortcut, with conflict warnings. */
function ShortcutDialog(props: { action: ActionDef; onClose: () => void }) {
  const [first, setFirst] = createSignal("");
  const [useSecond, setUseSecond] = createSignal(false);
  const [second, setSecond] = createSignal("");
  const text = () => (first() ? (useSecond() && second() ? `${first()} ${second()}` : first()) : "");
  return (
    <ShortcutDialogFrame title={tr("tools.keymapSettings.keyboardShortcut")} action={props.action} text={text()} hint={tr("tools.keymapSettings.pressTheKeysToUse")} onClose={props.onClose}>
      <div class="form-row">
        <span class="form-label">{tr("tools.keymapSettings.firstStrokeLabel")}</span>
        <ShortcutField value={first()} onChange={setFirst} autofocus />
      </div>
      <div class="form-row">
        <label class="form-label check">
          <input type="checkbox" checked={useSecond()} onChange={(e) => setUseSecond(e.currentTarget.checked)} />
          {tr("tools.keymapSettings.secondStrokeLabel")}
        </label>
        <ShortcutField value={second()} onChange={setSecond} disabled={!useSecond()} />
      </div>
    </ShortcutDialogFrame>
  );
}

/**
 * PHPStorm's Mouse Shortcut dialog: click (or turn the wheel) in the pad while holding the
 * modifiers. The keymap is suspended while it is open, so the clicks run nothing.
 */
function MouseShortcutDialog(props: { action: ActionDef; onClose: () => void }) {
  const [stroke, setStroke] = createSignal("");
  const [double, setDouble] = createSignal(false);
  const resume = suspendKeymap();
  onCleanup(resume);
  const record = (e: MouseEvent, wheel = false) => {
    e.preventDefault();
    e.stopPropagation();
    const input = mouseInputOf(e);
    const s = mouseStrokeOf(wheel ? input : { ...input, clickCount: double() ? 2 : 1, deltaY: undefined });
    if (s) setStroke(s);
  };
  // The desktop shell reports the side buttons on Linux itself (WebKitGTK hides them from
  // the page): record them while the dialog is open.
  const unlisten = listen<SideButtonClick>(SIDE_BUTTON_EVENT, ({ x: _x, y: _y, ...mouse }) => {
    const s = mouseStrokeOf({ ...mouse, clickCount: double() ? 2 : 1 });
    if (s) setStroke(s);
  });
  onCleanup(() => void unlisten.then((stop) => stop()));
  return (
    <ShortcutDialogFrame
      title={tr("tools.keymapSettings.mouseShortcut")}
      action={props.action}
      text={stroke()}
      hint={tr("tools.keymapSettings.clickOrTurnTheWheel")}
      onClose={props.onClose}
    >
      <div class="form-row mouse-kind">
        <label class="check">
          <input type="radio" name="mouse-kind" checked={!double()} onChange={() => setDouble(false)} />
          {tr("tools.keymapSettings.singleClick")}
        </label>
        <label class="check">
          <input type="radio" name="mouse-kind" checked={double()} onChange={() => setDouble(true)} />
          {tr("tools.keymapSettings.doubleClick")}
        </label>
      </div>
      <div
        class="mouse-pad"
        classList={{ empty: !stroke() }}
        data-testid="mouse-pad"
        onMouseDown={(e) => e.preventDefault()}
        onClick={(e) => record(e)}
        onAuxClick={(e) => record(e)}
        onContextMenu={(e) => e.preventDefault()}
        onWheel={(e) => record(e, true)}
      >
        <Mouse size={18} class="muted" />
        <span>{stroke() ? formatForKeymap(stroke()) : tr("tools.keymapSettings.clickPad")}</span>
      </div>
    </ShortcutDialogFrame>
  );
}

/** Group a keymap list for the selector: bundled keymaps, then the user's. */
function KeymapSelect() {
  const bundled = () => keymaps().filter((k) => !k.user);
  const user = () => keymaps().filter((k) => k.user);
  return (
    <div class="select-wrap">
      <select aria-label={tr("tools.keymapSettings.keymap")} value={keymapId()} onChange={(e) => setKeymapId(e.currentTarget.value)}>
        <For each={bundled()}>{(k) => <option value={k.id}>{k.title}</option>}</For>
        <Show when={user().length}>
          <optgroup label={tr("tools.keymapSettings.custom")}>
            <For each={user()}>{(k) => <option value={k.id}>{k.title}</option>}</For>
          </optgroup>
        </Show>
      </select>
      <ChevronDown size={14} class="select-chevron" />
    </div>
  );
}

const askKeymapName = (title: string, initial: string, confirm: string, except?: string) =>
  askName({ title, label: tr("common.nameLabel"), initial, confirm, validate: (n) => keymapNameProblem(n, except) });

/** A name like Rust's `free_name`, so the dialog suggests one that is free. */
function freeKeymapName(base: string): string {
  if (!keymapNameProblem(base)) return base;
  for (let n = 2; ; n++) if (!keymapNameProblem(`${base} ${n}`)) return `${base} ${n}`;
}

/** Ask for a name, then run `then` with it (nothing when cancelled). */
function withName(title: string, initial: string, confirm: string, then: (name: string) => unknown, except?: string): void {
  void askKeymapName(title, initial, confirm, except).then((n) => {
    if (n) void then(n);
  });
}

function keymapMenu(): MenuEntry[] {
  const km = activeKeymap();
  if (!km) return [];
  const entries: MenuEntry[] = [
    item("Duplicate…", () => withName("Duplicate Keymap", freeKeymapName(`${km.title} copy`), "OK", duplicateKeymap)),
    item("Derive…", () => withName("Derive Keymap", freeKeymapName(`${km.title} derived`), "OK", deriveKeymap)),
  ];
  if (km.user) {
    entries.push(
      item("Rename…", () => withName("Rename Keymap", km.id, "Rename", (n) => renameKeymap(km.id, n), km.id)),
      item("Delete", () => {
        void askConfirm({ title: tr("tools.keymapSettings.deleteKeymap"), message: tr("tools.keymapSettings.deleteTheKeymap", { title: km.title }), confirm: tr("common.delete") }).then((ok) => {
          if (ok) void deleteKeymap(km.id);
        });
      }),
    );
  }
  entries.push(separator, item("Restore Defaults", () => void resetKeymap(), undefined, { disabled: !modifiedCount() }));
  return entries;
}

function ContextMenu(props: {
  action: ActionDef;
  x: number;
  y: number;
  onClose: () => void;
  onAdd: () => void;
  onAddMouse: () => void;
}) {
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
      {item("Add Mouse Shortcut", props.onAddMouse)}
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
  const [mouseDialog, setMouseDialog] = createSignal<ActionDef | null>(null);
  const parentTitle = () => {
    const parent = activeKeymap()?.parent;
    return parent ? (keymaps().find((k) => k.id === parent)?.title ?? parent) : null;
  };

  // "Find Actions by Shortcut": the engine also finds chords starting with a single stroke.
  const [byShortcut, setByShortcut] = createSignal<Set<string> | null>(null);
  createEffect(() => {
    const sc = shortcutFilter();
    if (!sc) return setByShortcut(null);
    void findActionsByShortcut(sc).then((ids) => shortcutFilter() === sc && setByShortcut(new Set(ids)));
  });

  const visibleActions = createMemo(() => {
    const q = filter().trim().toLowerCase();
    const matches = shortcutFilter() ? byShortcut() : null;
    return allActions().filter((a) => {
      if (q && !a.title.toLowerCase().includes(q) && !a.id.toLowerCase().includes(q)) return false;
      return matches ? matches.has(a.id) : true;
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

  return (
    <div class="keymap-page">
      <div class="km-top">
        <span class="form-label">{tr("tools.keymapSettings.keymapLabel")}</span>
        <KeymapSelect />
        <button
          class="icon-btn"
          title={tr("tools.keymapSettings.showKeymapActions")}
          onClick={(e) => {
            const r = e.currentTarget.getBoundingClientRect();
            showContextMenu(keymapMenu(), r.left, r.bottom + 2);
          }}
        >
          <Settings size={15} />
        </button>
        <Show when={parentTitle()}>
          <span class="muted km-parent">{tr("tools.keymapSettings.basedOn", { parentTitle: parentTitle() })}</span>
        </Show>
        <Show when={modifiedCount()}>
          <span class="muted km-modified">{modifiedCount()} modified</span>
          <button class="link-btn" onClick={() => void resetKeymap()}>
            <RotateCcw size={12} /> {tr("common.reset")}
          </button>
        </Show>
      </div>

      <div class="km-toolbar">
        <button class="icon-btn" title={tr("common.expandAll2")} onClick={() => setExpanded(allGroups())}>
          <ChevronsUpDown size={15} />
        </button>
        <button class="icon-btn" title={tr("common.collapseAll2")} onClick={() => setExpanded(new Set<string>())}>
          <ChevronsDownUp size={15} />
        </button>
        <button
          class="icon-btn"
          title={tr("tools.keymapSettings.editShortcuts")}
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
          <input placeholder={tr("tools.keymapSettings.searchActions")} value={filter()} onInput={(e) => setFilter(e.currentTarget.value)} />
          <Show when={filter()}>
            <button class="icon-btn" title={tr("common.clear")} onClick={() => setFilter("")}>
              <X size={12} />
            </button>
          </Show>
        </div>
        <div class="km-find-wrap">
          <button
            class="icon-btn"
            classList={{ active: findOpen() || !!shortcutFilter() }}
            title={tr("tools.keymapSettings.findActionsByShortcut")}
            onClick={() => setFindOpen((v) => !v)}
          >
            <Keyboard size={16} />
          </button>
          <Show when={findOpen()}>
            <div class="popup km-find-popup">
              <div class="km-find-title">{tr("tools.keymapSettings.findActionsByShortcut")}</div>
              <ShortcutField
                value={shortcutFilter()}
                onChange={(s) => setShortcutFilter(s)}
                placeholder={tr("tools.keymapSettings.pressAShortcutEllipsis")}
                autofocus
              />
              <div class="km-find-hint muted">{tr("tools.keymapSettings.theListShowsActionsBound")}</div>
            </div>
          </Show>
        </div>
      </div>

      <Show when={shortcutFilter()}>
        <div class="km-filter-chip">
          <span class="muted">{tr("tools.keymapSettings.shortcutLabel")}</span>
          <span class="kbd">{formatForKeymap(shortcutFilter())}</span>
          <button class="icon-btn" title={tr("tools.keymapSettings.clearShortcutFilter")} onClick={() => setShortcutFilter("")}>
            <X size={12} />
          </button>
        </div>
      </Show>

      <div class="km-tree" tabIndex={0} ref={treeEl} onKeyDown={onTreeKey}>
        <Show when={rows().length} fallback={<div class="se-empty">{tr("common.nothingToShow")}</div>}>
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
              const list = () => displayShortcutsOf(a.id);
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
                    <For each={list()}>{(s) => <span class="km-shortcut">{s}</span>}</For>
                  </span>
                </div>
              );
            }}
          </For>
        </Show>
      </div>

      <div class="km-note muted">
        {tr("tools.keymapSettings.shortcutsAreSavedAsYou")}
      </div>

      <Show when={menu()}>
        {(m) => (
          <ContextMenu
            {...m()}
            onClose={() => setMenu(null)}
            onAdd={() => setDialog(m().action)}
            onAddMouse={() => setMouseDialog(m().action)}
          />
        )}
      </Show>
      <Show when={dialog()}>{(a) => <ShortcutDialog action={a()} onClose={() => setDialog(null)} />}</Show>
      <Show when={mouseDialog()}>{(a) => <MouseShortcutDialog action={a()} onClose={() => setMouseDialog(null)} />}</Show>
    </div>
  );
}
