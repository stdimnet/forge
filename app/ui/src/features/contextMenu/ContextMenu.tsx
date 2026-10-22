/**
 * Renders the open menu (context menus and the main menu): keyboard navigation, submenus,
 * mnemonics, disabled and checkable entries, viewport clamping.
 */
import { For, Show, createEffect, createSignal, onCleanup, onMount } from "solid-js";
import { Dynamic } from "solid-js/web";
import { Check, ChevronRight } from "lucide-solid";
import { runAction, suspendKeymap } from "@/keymap";
import { closeContextMenu, openMenu, type ResolvedEntry, type ResolvedItem } from "./state";

const MARGIN = 6;

/** Keep a box of `size` that starts at `pos` inside `[0, limit]`. */
export function clampToViewport(pos: number, size: number, limit: number): number {
  return Math.max(MARGIN, Math.min(pos, limit - size - MARGIN));
}

const isItem = (e: ResolvedEntry | undefined): e is ResolvedItem => !!e && e !== "separator";
const selectable = (e: ResolvedEntry | undefined): e is ResolvedItem => isItem(e) && !e.disabled;

/** Lower-case mnemonic letter of an item, if it has one. */
export function mnemonicOf(item: ResolvedItem): string | undefined {
  return item.mnemonic == null ? undefined : Array.from(item.label)[item.mnemonic]?.toLowerCase();
}

/**
 * Indexes of the enabled items whose mnemonic is `key`. One match is picked, several are
 * cycled through (like JetBrains menus).
 */
export function mnemonicMatches(entries: ResolvedEntry[], key: string): number[] {
  const k = key.toLowerCase();
  return entries.flatMap((e, i) => (selectable(e) && mnemonicOf(e) === k ? [i] : []));
}

/** The label with its mnemonic letter underlined. */
export function MnemonicLabel(props: { label: string; mnemonic?: number }) {
  const parts = () => {
    const chars = Array.from(props.label.replace(/…$/, "..."));
    const at = props.mnemonic;
    if (at == null || at >= chars.length) return null;
    return [chars.slice(0, at).join(""), chars[at], chars.slice(at + 1).join("")] as const;
  };
  return (
    <Show when={parts()} fallback={props.label.replace(/…$/, "...")}>
      {(p) => (
        <>
          {p()[0]}
          <u class="mnemonic">{p()[1]}</u>
          {p()[2]}
        </>
      )}
    </Show>
  );
}

interface ListProps {
  entries: ResolvedEntry[];
  x: number;
  y: number;
  /** Pixel rect of the parent row; submenus flip to the left when they do not fit on the right. */
  anchor?: DOMRect;
  focused: boolean;
  selectFirst?: boolean;
  class?: string;
  /** `y` is the bottom edge (the menu opens upwards). */
  above?: boolean;
  onPick: (item: ResolvedItem) => void;
  onLeave?: () => void;
  onCross?: (dir: 1 | -1) => void;
  /** This list handles Right on leaf items of its submenus by opening its next submenu. */
  crossLevel?: boolean;
}

function MenuList(props: ListProps) {
  let root!: HTMLDivElement;
  const [pos, setPos] = createSignal({ left: props.x, top: props.y });
  const [active, setActive] = createSignal(-1);
  const [opened, setOpened] = createSignal(-1);
  /** The open submenu was opened from the keyboard: it selects its first item. */
  const [byKey, setByKey] = createSignal(false);

  const indexes = () => props.entries.flatMap((e, i) => (selectable(e) ? [i] : []));
  const hasChecks = () => props.entries.some((e) => isItem(e) && e.checkable);

  onMount(() => {
    const r = root.getBoundingClientRect();
    let left = props.x;
    if (props.anchor && left + r.width > window.innerWidth - MARGIN) left = props.anchor.left - r.width + 2;
    setPos({ left: clampToViewport(left, r.width, window.innerWidth), top: clampToViewport(props.above ? props.y - r.height : props.y, r.height, window.innerHeight) });
    if (props.selectFirst) setActive(indexes()[0] ?? -1);
  });
  createEffect(() => {
    if (props.focused) root.focus({ preventScroll: true });
  });

  const move = (dir: 1 | -1) => {
    const list = indexes();
    if (!list.length) return;
    const at = list.indexOf(active());
    const next = at < 0 ? (dir === 1 ? 0 : list.length - 1) : (at + dir + list.length) % list.length;
    setActive(list[next]);
  };
  const activate = (i: number, fromKey: boolean) => {
    const e = props.entries[i];
    if (!selectable(e)) return;
    setActive(i);
    if (e.kind === "submenu") {
      setByKey(fromKey);
      setOpened(i);
    } else props.onPick(e);
  };
  /** Open the next/previous submenu of this list (Right on a leaf of an open submenu). */
  const crossTo = (dir: 1 | -1) => {
    const subs = indexes().filter((i) => (props.entries[i] as ResolvedItem).kind === "submenu");
    if (!subs.length) return;
    const at = subs.indexOf(opened());
    const next = subs[(at + dir + subs.length) % subs.length];
    setActive(next);
    setByKey(true);
    setOpened(-1);
    queueMicrotask(() => setOpened(next));
  };

  const onKeyDown = (ev: KeyboardEvent) => {
    const key = ev.key;
    const current = props.entries[active()];
    if (key === "ArrowDown") move(1);
    else if (key === "ArrowUp") move(-1);
    else if (key === "Home") setActive(indexes()[0] ?? -1);
    else if (key === "End") setActive(indexes().at(-1) ?? -1);
    else if (key === "Enter" || key === " ") activate(active(), true);
    else if (key === "ArrowRight") {
      if (selectable(current) && current.kind === "submenu") activate(active(), true);
      else if (props.onCross) props.onCross(1);
      else return;
    } else if (key === "ArrowLeft") {
      if (props.onLeave) props.onLeave();
      else if (props.onCross) props.onCross(-1);
      else return;
    } else if (key === "Escape") {
      if (props.onLeave) props.onLeave();
      else closeContextMenu();
    } else if (key.length === 1 && !ev.ctrlKey && !ev.metaKey) {
      // Mnemonics: the letter alone or with Alt, like JetBrains menus.
      const matches = mnemonicMatches(props.entries, ev.code.startsWith("Key") ? ev.code.slice(3) : key);
      if (matches.length === 1) activate(matches[0], true);
      else if (matches.length > 1) setActive(matches[(matches.indexOf(active()) + 1) % matches.length]);
      else return;
    } else return;
    ev.preventDefault();
    ev.stopPropagation();
  };

  return (
    <div
      ref={root}
      classList={{ "context-menu": true, "with-checks": hasChecks(), ...(props.class ? { [props.class]: true } : {}) }}
      role="menu"
      tabindex="-1"
      style={{ left: `${pos().left}px`, top: `${pos().top}px` }}
      onKeyDown={onKeyDown}
      onContextMenu={(e) => e.preventDefault()}
    >
      <For each={props.entries}>
        {(entry, i) => (
          <Show when={entry !== "separator"} fallback={<div class="context-menu-separator" role="separator" />}>
            {(() => {
              const item = entry as ResolvedItem;
              let row!: HTMLDivElement;
              return (
                <div
                  ref={row}
                  class="context-menu-item"
                  classList={{ active: active() === i(), open: opened() === i(), disabled: !!item.disabled }}
                  role={item.checkable ? "menuitemcheckbox" : "menuitem"}
                  aria-disabled={item.disabled ? "true" : undefined}
                  aria-checked={item.checkable ? !!item.checked : undefined}
                  aria-haspopup={item.kind === "submenu" ? "menu" : undefined}
                  onMouseEnter={() => {
                    if (item.disabled) {
                      setActive(-1);
                      setOpened(-1);
                      return;
                    }
                    setActive(i());
                    setByKey(false);
                    setOpened(item.kind === "submenu" ? i() : -1);
                  }}
                  onClick={() => activate(i(), false)}
                >
                  <Show when={hasChecks()}>
                    <span class="context-menu-check">
                      <Show when={item.checked}>
                        <Check size={13} />
                      </Show>
                    </span>
                  </Show>
                  <Show when={item.icon}>
                    {(icon) => <Dynamic component={icon()} size={14} stroke-width={1.75} class="context-menu-icon" />}
                  </Show>
                  <span class="context-menu-label">
                    <MnemonicLabel label={item.label} mnemonic={item.mnemonic} />
                    <Show when={item.detail}>
                      <span class="context-menu-detail">{item.detail}</span>
                    </Show>
                  </span>
                  <Show when={item.kind === "action" && item.shortcut}>
                    <span class="context-menu-shortcut">{item.shortcut}</span>
                  </Show>
                  <Show when={item.kind === "submenu"}>
                    <ChevronRight size={13} class="context-menu-chevron" />
                    <Show when={opened() === i()}>
                      <MenuList
                        entries={item.items!}
                        x={root.getBoundingClientRect().right - 1}
                        y={row.getBoundingClientRect().top - 5}
                        anchor={row.getBoundingClientRect()}
                        focused={opened() === i() && active() === i() && props.focused}
                        selectFirst={byKey()}
                        onPick={props.onPick}
                        onLeave={() => {
                          setOpened(-1);
                          root.focus({ preventScroll: true });
                        }}
                        onCross={props.crossLevel ? crossTo : props.onCross}
                      />
                    </Show>
                  </Show>
                </div>
              );
            })()}
          </Show>
        )}
      </For>
    </div>
  );
}

export function ContextMenuHost() {
  return (
    <Show when={openMenu()} keyed>
      {(menu) => {
        const release = suspendKeymap();
        const close = (e: Event) => {
          const target = e.target as Element;
          if (target.closest?.(".context-menu")) return;
          if (menu.keepOpenOn && target.closest?.(menu.keepOpenOn)) return;
          closeContextMenu();
        };
        document.addEventListener("mousedown", close, true);
        window.addEventListener("blur", closeContextMenu);
        window.addEventListener("resize", closeContextMenu);
        onCleanup(() => {
          release();
          document.removeEventListener("mousedown", close, true);
          window.removeEventListener("blur", closeContextMenu);
          window.removeEventListener("resize", closeContextMenu);
        });
        return (
          <MenuList
            entries={menu.entries}
            x={menu.x}
            y={menu.y}
            focused
            selectFirst={menu.selectFirst}
            above={menu.above}
            class={menu.class}
            crossLevel={menu.crossTopLevel}
            onCross={menu.onCross}
            onPick={(item) => {
              if (item.disabled) return;
              closeContextMenu();
              if (item.run) item.run();
              else void runAction(item.action!);
            }}
          />
        );
      }}
    </Show>
  );
}
