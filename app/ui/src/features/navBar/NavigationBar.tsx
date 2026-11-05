/**
 * The Navigation Bar like PHPStorm's: `project › app › Models › User.php › User › posts()`
 * above the editor, in the status bar, or as a popup (Alt+Home while it is hidden). Every
 * segment opens a dropdown with its children (a folder's entries, a file's or class's
 * members) and speed search; the keyboard moves between segments (← →), opens the dropdown
 * (↓) and opens files (Enter). Segments come from Rust (`forge_codeintel::navbar`), the
 * dropdown's matching from `forge-fuzzy`.
 */
import { t, t as tr } from "@/i18n";
import { createEffect, createMemo, createResource, createSignal, For, Match, on, onCleanup, Show, Switch } from "solid-js";
import { Layer } from "@/shell/layers";
import { ChevronRight, Search } from "lucide-solid";
import { activeTab, type EditorTab } from "@/state/editor";
import { FileIcon } from "@/shell/FileIcon";
import { SymbolIcon } from "@/features/php/SymbolIcon";
import { revealStructureNode } from "@/features/php/StructureView";
import { symbolKindOf } from "@/features/codeStructure/breadcrumbs";
import { createFuzzyFilter } from "@/features/search/fuzzyFilter";
import { Highlight } from "@/features/search/SearchEverywhere";
import { openFileAt, viewFor } from "@/editor/views";
import {
  childrenOf,
  clearNavBarFocusRequest,
  focusRequest,
  navBarLocation,
  navBarMembers,
  segmentsOf,
  setFloatingNavBar,
  type NavItem,
  type NavSegment,
} from "./model";

export type NavBarPlacement = "top" | "statusBar" | "floating";

interface PopupState {
  /** The segment whose children are listed. */
  index: number;
  /** Label of the entry to select first (the next segment on the path). */
  preselect?: string;
}

function SegmentIcon(props: { seg: NavSegment }) {
  return (
    <Switch>
      <Match when={props.seg.kind === "project" || props.seg.kind === "directory"}>
        <FileIcon name={props.seg.label} dir size={14} />
      </Match>
      <Match when={props.seg.kind === "file"}>
        <FileIcon name={props.seg.label} size={14} />
      </Match>
      <Match when={props.seg.scopeKind}>{(k) => <SymbolIcon kind={symbolKindOf(k())} />}</Match>
    </Switch>
  );
}

function ItemIcon(props: { item: NavItem }) {
  return (
    <Switch>
      <Match when={props.item.kind === "directory"}>
        <FileIcon name={props.item.label} path={props.item.path} dir size={14} />
      </Match>
      <Match when={props.item.kind === "file"}>
        <FileIcon name={props.item.label} path={props.item.path} size={14} />
      </Match>
      <Match when={props.item.node}>
        {(n) => <SymbolIcon kind={n().kind} visibility={n().visibility} modifiers={n().modifiers} />}
      </Match>
    </Switch>
  );
}

export function NavigationBar(props: { placement: NavBarPlacement; group?: number; tab?: () => EditorTab | undefined }) {
  let root!: HTMLDivElement;
  let popupEl: HTMLDivElement | undefined;
  const tab = () => (props.tab ? props.tab() : activeTab());
  // With the bar on top, the status bar keeps the plain path (project, folders, file).
  const current = createMemo(() =>
    segmentsOf(tab(), props.group, navBarMembers() && !(props.placement === "statusBar" && navBarLocation() === "top")),
  );
  /** Segments while browsing folders from a dropdown (until a file opens or the bar is left). */
  const [browse, setBrowse] = createSignal<NavSegment[] | null>(null);
  const segments = () => browse() ?? current().segments;
  const [focused, setFocused] = createSignal(false);
  const [sel, setSel] = createSignal(0);
  const [popup, setPopup] = createSignal<PopupState | null>(null);
  const [query, setQuery] = createSignal("");
  const [index, setIndex] = createSignal(0);

  const [items] = createResource(
    () => {
      const p = popup();
      const seg = p ? segments()[p.index] : undefined;
      return p && seg ? { seg, preselect: p.preselect } : null;
    },
    async (k) => childrenOf(k.seg, tab()).catch(() => [] as NavItem[]),
  );
  const hits = createFuzzyFilter(() => ({ query: query(), candidates: (items() ?? []).map((i) => i.label) }));
  const shown = createMemo(() =>
    hits()
      .filter((h) => h.index < (items() ?? []).length)
      .map((h) => ({ item: items()![h.index], positions: h.positions })),
  );

  // Select the entry on the path when a dropdown's entries arrive.
  createEffect(
    on(items, (list) => {
      const pre = popup()?.preselect;
      const at = pre ? (list ?? []).findIndex((i) => i.label === pre) : -1;
      setIndex(Math.max(0, at));
    }),
  );
  createEffect(on(query, () => setIndex(0), { defer: true }));
  createEffect(() => {
    popupEl?.querySelector(`[data-item="${index()}"]`)?.scrollIntoView({ block: "nearest" });
  });
  // Leave the browsing state when the active file changes.
  createEffect(on(() => tab()?.path, () => setBrowse(null), { defer: true }));

  const openPopup = (i: number, preselect?: string) => {
    const segs = segments();
    if (i < 0 || i >= segs.length) return;
    setSel(i);
    setQuery("");
    setPopup({ index: i, preselect: preselect ?? segs[i + 1]?.label });
  };
  const closePopup = () => {
    setPopup(null);
    setQuery("");
  };

  /** Leave the bar: back to the editor. */
  const leave = () => {
    closePopup();
    setBrowse(null);
    setFocused(false);
    if (props.placement === "floating") setFloatingNavBar(null);
    const t = tab();
    const view = t ? viewFor(t.path, props.group) : undefined;
    if (view) view.focus();
    else root?.blur();
  };

  const choose = async (item: NavItem | undefined) => {
    const p = popup();
    if (!item || !p) return;
    if (item.kind === "directory") {
      const segs = segments().slice(0, p.index + 1);
      segs.push({ kind: "directory", label: item.label, path: item.path });
      setBrowse(segs);
      openPopup(segs.length - 1);
      return;
    }
    leave();
    if (item.kind === "file") await openFileAt(item.path);
    else if (item.node) revealStructureNode(item.path, item.node);
  };

  /** The selected segment itself: files open, folders and members show their dropdown. */
  const activate = (i: number) => {
    const seg = segments()[i];
    if (!seg) return;
    if (seg.kind === "file" && seg.path !== tab()?.path) {
      leave();
      void openFileAt(seg.path);
    } else if (seg.kind === "file" && i === segments().length - 1) {
      leave();
    } else openPopup(i);
  };

  // Alt+Home: focus this bar with the active file's folder open, the file selected.
  createEffect(
    on(focusRequest, (req) => {
      if (!req || req.placement !== props.placement) return;
      if (props.placement !== "statusBar" && req.group !== undefined && req.group !== props.group) return;
      clearNavBarFocusRequest();
      setBrowse(null);
      const segs = segments();
      const file = segs.findIndex((s) => s.kind === "file");
      const target = file > 0 ? file - 1 : Math.max(0, segs.length - 1);
      setFocused(true);
      queueMicrotask(() => {
        root.focus();
        openPopup(target, file > 0 ? segs[file].label : undefined);
      });
    }),
  );

  const away = (e: MouseEvent) => {
    const t = e.target as Node;
    if (root?.contains(t) || popupEl?.contains(t)) return;
    closePopup();
    setBrowse(null);
    if (props.placement === "floating") setFloatingNavBar(null);
  };
  createEffect(() => {
    if (popup() || props.placement === "floating") {
      window.addEventListener("mousedown", away, true);
      onCleanup(() => window.removeEventListener("mousedown", away, true));
    }
  });

  // The bar's own keys while it has focus (a list widget, not shortcuts).
  function onKeyDown(e: KeyboardEvent) {
    const n = segments().length;
    const p = popup();
    const count = shown().length;
    if (e.ctrlKey || e.metaKey || e.altKey) return;
    switch (e.key) {
      case "ArrowLeft":
        if (p) openPopup(Math.max(0, p.index - 1));
        else setSel((i) => Math.max(0, i - 1));
        break;
      case "ArrowRight":
        if (p) openPopup(Math.min(n - 1, p.index + 1));
        else setSel((i) => Math.min(n - 1, i + 1));
        break;
      case "ArrowDown":
        if (p) setIndex((i) => (count ? (i + 1) % count : 0));
        else openPopup(sel());
        break;
      case "ArrowUp":
        if (p) setIndex((i) => (count ? (i + count - 1) % count : 0));
        break;
      case "Home":
        if (!p) setSel(0);
        else return;
        break;
      case "End":
        if (!p) setSel(n - 1);
        else return;
        break;
      case "Enter":
        if (p) void choose(shown()[index()]?.item);
        else activate(sel());
        break;
      case "Escape":
        if (p && query()) setQuery("");
        else if (p && !browse()) closePopup();
        else leave();
        break;
      case "Backspace":
        if (p) setQuery((q) => q.slice(0, -1));
        else return;
        break;
      default:
        if (p && e.key.length === 1) setQuery((q) => q + e.key);
        else return;
    }
    e.preventDefault();
    e.stopPropagation();
  }

  const anchorRect = () => {
    const p = popup();
    if (!p) return null;
    void segments();
    return root?.querySelector(`[data-seg="${p.index}"]`)?.getBoundingClientRect() ?? root?.getBoundingClientRect() ?? null;
  };
  const up = () => props.placement === "statusBar";

  return (
    <div
      class={`navbar navbar-${props.placement}`}
      classList={{ focused: focused() }}
      ref={root}
      tabIndex={-1}
      role="navigation"
      aria-label={t(props.placement === "statusBar" ? "a11y.navbar.statusBar" : "a11y.navbar")}
      data-group={props.group}
      onKeyDown={onKeyDown}
      onFocusIn={() => setFocused(true)}
      onFocusOut={(e) => {
        const next = e.relatedTarget as Node | null;
        if (next && (root.contains(next) || popupEl?.contains(next))) return;
        setFocused(false);
      }}
    >
      <Show
        when={!current().plain || browse()}
        fallback={
          <For each={current().plain}>
            {(p, i) => (
              <>
                <Show when={i() > 0}>
                  <ChevronRight size={12} class="navbar-sep" />
                </Show>
                <span class="navbar-seg plain">{p}</span>
              </>
            )}
          </For>
        }
      >
        <For each={segments()}>
          {(seg, i) => (
            <>
              <Show when={i() > 0}>
                <ChevronRight size={12} class="navbar-sep" />
              </Show>
              <span
                class={`navbar-seg navbar-${seg.kind}`}
                classList={{
                  selected: focused() && sel() === i(),
                  open: popup()?.index === i(),
                  last: i() === segments().length - 1,
                }}
                data-seg={i()}
                title={seg.path || seg.label}
                onMouseDown={(e) => e.preventDefault()}
                onClick={() => {
                  root.focus();
                  if (popup()?.index === i()) closePopup();
                  else openPopup(i());
                }}
              >
                <SegmentIcon seg={seg} />
                <span class="navbar-label">{seg.label}</span>
              </span>
            </>
          )}
        </For>
      </Show>
      <Show when={popup() && anchorRect()}>
        {(rect) => (
          <Layer kind="popup">
            <div
              class="navbar-popup"
              ref={popupEl}
              role="listbox"
              aria-label={segments()[popup()!.index]?.label}
              style={{
                left: `${Math.max(4, Math.min(rect().left - 4, window.innerWidth - 324))}px`,
                ...(up() ? { bottom: `${window.innerHeight - rect().top + 4}px` } : { top: `${rect().bottom + 4}px` }),
              }}
              onMouseDown={(e) => {
                e.preventDefault();
                root.focus();
              }}
            >
              <Show when={query()}>
                <div class="navbar-popup-search">
                  <Search size={12} />
                  {query()}
                </div>
              </Show>
              <Show
                when={shown().length}
                fallback={<div class="navbar-popup-empty">{items.loading ? tr("common.loadingEllipsis") : query() ? tr("common.nothingFound") : tr("tools.navBar.empty")}</div>}
              >
                <For each={shown()}>
                  {(entry, i) => (
                    <div
                      class="navbar-item"
                      classList={{ selected: index() === i(), ignored: !!entry.item.ignored }}
                      role="option"
                      aria-selected={index() === i()}
                      data-item={i()}
                      onMouseMove={() => setIndex(i())}
                      onClick={() => void choose(entry.item)}
                    >
                      <ItemIcon item={entry.item} />
                      <span class="navbar-item-label">
                        <Highlight text={entry.item.label} positions={entry.positions} />
                      </span>
                      <Show when={entry.item.node?.detail}>
                        <span class="navbar-item-detail">{entry.item.node!.detail}</span>
                      </Show>
                      <Show when={entry.item.kind === "directory"}>
                        <ChevronRight size={12} class="navbar-item-more" />
                      </Show>
                    </div>
                  )}
                </For>
              </Show>
            </div>
          </Layer>
        )}
      </Show>
    </div>
  );
}
