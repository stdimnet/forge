/** Renders the open context menu: keyboard navigation, submenus, viewport clamping. */
import { For, Show, createEffect, createSignal, onCleanup, onMount } from "solid-js";
import { ChevronRight } from "lucide-solid";
import { runAction, suspendKeymap } from "@/keymap";
import { closeContextMenu, openMenu, type ResolvedEntry, type ResolvedItem } from "./state";

const MARGIN = 6;

/** Keep a box of `size` that starts at `pos` inside `[0, limit]`. */
export function clampToViewport(pos: number, size: number, limit: number): number {
  return Math.max(MARGIN, Math.min(pos, limit - size - MARGIN));
}

function MenuList(props: {
  entries: ResolvedEntry[];
  x: number;
  y: number;
  /** Pixel rect of the parent row; submenus flip to the left when they do not fit on the right. */
  anchor?: DOMRect;
  focused: boolean;
  onPick: (action: string) => void;
  onLeave?: () => void;
}) {
  let root!: HTMLDivElement;
  const [pos, setPos] = createSignal({ left: props.x, top: props.y });
  const [active, setActive] = createSignal(-1);
  const [opened, setOpened] = createSignal(-1);

  const items = () => props.entries.map((e, i) => [e, i] as const).filter((p): p is [ResolvedItem, number] => p[0] !== "separator");

  onMount(() => {
    const r = root.getBoundingClientRect();
    let left = props.x;
    if (props.anchor && left + r.width > window.innerWidth - MARGIN) left = props.anchor.left - r.width + 2;
    setPos({ left: clampToViewport(left, r.width, window.innerWidth), top: clampToViewport(props.y, r.height, window.innerHeight) });
  });
  createEffect(() => {
    if (props.focused) root.focus({ preventScroll: true });
  });

  const move = (dir: 1 | -1) => {
    const list = items();
    if (!list.length) return;
    const at = list.findIndex(([, i]) => i === active());
    const next = at < 0 ? (dir === 1 ? 0 : list.length - 1) : (at + dir + list.length) % list.length;
    setActive(list[next][1]);
  };
  const activate = (i: number) => {
    const e = props.entries[i];
    if (e === "separator" || !e) return;
    if (e.kind === "submenu") setOpened(i);
    else props.onPick(e.action!);
  };

  const onKeyDown = (ev: KeyboardEvent) => {
    const key = ev.key;
    if (key === "ArrowDown") move(1);
    else if (key === "ArrowUp") move(-1);
    else if (key === "Home") setActive(items()[0]?.[1] ?? -1);
    else if (key === "End") setActive(items().at(-1)?.[1] ?? -1);
    else if (key === "Enter" || key === " " || key === "ArrowRight") {
      const e = props.entries[active()];
      if (key === "ArrowRight" && (e === "separator" || e?.kind !== "submenu")) return;
      activate(active());
    } else if (key === "ArrowLeft" && props.onLeave) props.onLeave();
    else if (key === "Escape") {
      if (props.onLeave) props.onLeave();
      else closeContextMenu();
    } else return;
    ev.preventDefault();
    ev.stopPropagation();
  };

  return (
    <div
      ref={root}
      class="context-menu"
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
                  classList={{ active: active() === i(), open: opened() === i() }}
                  role="menuitem"
                  onMouseEnter={() => {
                    setActive(i());
                    setOpened(item.kind === "submenu" ? i() : -1);
                  }}
                  onClick={() => activate(i())}
                >
                  <span class="context-menu-label">{item.label.replace(/…$/, "...")}</span>
                  <Show when={item.kind === "action" && item.shortcut}>
                    <span class="context-menu-shortcut">{item.shortcut}</span>
                  </Show>
                  <Show when={item.kind === "submenu"}>
                    <ChevronRight size={13} class="context-menu-chevron" />
                    <Show when={opened() === i()}>
                      <MenuList
                        entries={item.items!}
                        x={row.getBoundingClientRect().right - 2}
                        y={row.getBoundingClientRect().top - 5}
                        anchor={row.getBoundingClientRect()}
                        focused={opened() === i() && active() === i() && props.focused}
                        onPick={props.onPick}
                        onLeave={() => setOpened(-1)}
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
          if (!(e.target as Element).closest?.(".context-menu")) closeContextMenu();
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
            onPick={(id) => {
              closeContextMenu();
              void runAction(id);
            }}
          />
        );
      }}
    </Show>
  );
}
