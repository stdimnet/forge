/** Small context menu used by the Database tree and the data grid. */
import { For, onCleanup, onMount, Show, type JSX } from "solid-js";

export interface MenuItem {
  label: string;
  hint?: string;
  icon?: JSX.Element;
  disabled?: boolean;
  danger?: boolean;
  run?: () => void;
  separator?: boolean;
}

export function ContextMenu(props: { x: number; y: number; items: MenuItem[]; onClose: () => void }) {
  let el!: HTMLDivElement;
  const away = (e: MouseEvent) => {
    if (!el.contains(e.target as Node)) props.onClose();
  };
  onMount(() => {
    window.addEventListener("mousedown", away, true);
    // Keep the menu on screen.
    const r = el.getBoundingClientRect();
    if (r.bottom > window.innerHeight - 8) el.style.top = `${Math.max(8, window.innerHeight - r.height - 8)}px`;
    if (r.right > window.innerWidth - 8) el.style.left = `${Math.max(8, window.innerWidth - r.width - 8)}px`;
    el.focus();
  });
  onCleanup(() => window.removeEventListener("mousedown", away, true));
  return (
    <div
      class="db-menu"
      ref={el}
      tabIndex={-1}
      role="menu"
      style={{ left: `${props.x}px`, top: `${props.y}px` }}
      onKeyDown={(e) => {
        if (e.key === "Escape") {
          e.stopPropagation();
          props.onClose();
        }
      }}
      onContextMenu={(e) => e.preventDefault()}
    >
      <For each={props.items}>
        {(item) => (
          <Show when={!item.separator} fallback={<div class="db-menu-sep" />}>
            <button
              class="db-menu-item"
              classList={{ danger: item.danger }}
              role="menuitem"
              disabled={item.disabled}
              onClick={() => {
                props.onClose();
                item.run?.();
              }}
            >
              <span class="db-menu-icon">{item.icon}</span>
              <span class="db-menu-label">{item.label}</span>
              <Show when={item.hint}>
                <span class="db-menu-hint">{item.hint}</span>
              </Show>
            </button>
          </Show>
        )}
      </For>
    </div>
  );
}
