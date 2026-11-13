/** A small context/popup menu at a screen position (uses the shared `.context-menu` chrome). */
import { createSignal, For, onCleanup, onMount, Show, type JSX } from "solid-js";
import { Dynamic } from "solid-js/web";
import { Layer } from "@/shell/layers";
import type { IconComponent } from "./types";

export interface MenuItem {
  label?: string;
  icon?: IconComponent;
  iconColor?: string;
  shortcut?: string;
  disabled?: boolean;
  separator?: boolean;
  run?: () => void;
}

export interface MenuRequest {
  x: number;
  y: number;
  items: MenuItem[];
}

const [menu, setMenu] = createSignal<MenuRequest | null>(null);

export const openMenu = (req: MenuRequest) => setMenu(req);
export const closeMenu = () => setMenu(null);

/** Mount once (the run feature's overlay host renders it). */
export function PopupMenuHost(): JSX.Element {
  return (
    <Show when={menu()}>
      {(m) => <PopupMenu request={m()} />}
    </Show>
  );
}

function PopupMenu(props: { request: MenuRequest }) {
  let el!: HTMLDivElement;
  const onDoc = (e: MouseEvent) => {
    if (!(e.target as Element).closest(".run-popup-menu")) closeMenu();
  };
  const [pos, setPos] = createSignal({ x: props.request.x, y: props.request.y });
  onMount(() => {
    document.addEventListener("mousedown", onDoc, true);
    // Keep the menu on screen.
    const r = el.getBoundingClientRect();
    setPos({
      x: Math.max(4, Math.min(props.request.x, window.innerWidth - r.width - 4)),
      y: Math.max(4, Math.min(props.request.y, window.innerHeight - r.height - 4)),
    });
    el.focus();
  });
  onCleanup(() => document.removeEventListener("mousedown", onDoc, true));
  return (
    <Layer kind="context-menu">
      <div
        ref={el}
        class="context-menu popup run-popup-menu"
        tabIndex={-1}
        role="menu"
        style={{ left: `${pos().x}px`, top: `${pos().y}px` }}
        onKeyDown={(e) => {
          if (e.key === "Escape") {
            e.stopPropagation();
            closeMenu();
          }
        }}
      >
        <For each={props.request.items}>
          {(item) =>
            item.separator ? (
              <div class="context-sep" />
            ) : (
              <button
                class="context-item run-menu-item"
                role="menuitem"
                disabled={item.disabled}
                onClick={() => {
                  closeMenu();
                  item.run?.();
                }}
              >
                <span class="run-menu-icon" style={{ color: item.iconColor }}>
                  <Show when={item.icon}>{(I) => <Dynamic component={I()} size={14} />}</Show>
                </span>
                <span class="run-menu-label">{item.label}</span>
                <Show when={item.shortcut}>
                  <span class="kbd">{item.shortcut}</span>
                </Show>
              </button>
            )
          }
        </For>
      </div>
    </Layer>
  );
}

