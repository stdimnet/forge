/** Small context menu used by the Database tree and the data grid; entries with `items`
 * open a submenu (SQL Scripts ›, Import/Export ›). */
import { createSignal, For, onCleanup, onMount, Show, type JSX } from "solid-js";
import { ChevronRight } from "lucide-solid";

export interface MenuItem {
  label: string;
  hint?: string;
  icon?: JSX.Element;
  disabled?: boolean;
  danger?: boolean;
  run?: () => void;
  separator?: boolean;
  /** A submenu. */
  items?: MenuItem[];
}

export function ContextMenu(props: { x: number; y: number; items: MenuItem[]; onClose: () => void }) {
  let el!: HTMLDivElement;
  const away = (e: MouseEvent) => {
    if (!el.contains(e.target as Node)) props.onClose();
  };
  onMount(() => {
    window.addEventListener("mousedown", away, true);
    el.focus();
  });
  onCleanup(() => window.removeEventListener("mousedown", away, true));
  return (
    <div ref={el} onContextMenu={(e) => e.preventDefault()}>
      <MenuPanel x={props.x} y={props.y} items={props.items} onClose={props.onClose} focus />
    </div>
  );
}

function MenuPanel(props: { x: number; y: number; items: MenuItem[]; onClose: () => void; focus?: boolean; sub?: boolean; flipX?: number }) {
  let el!: HTMLDivElement;
  const [open, setOpen] = createSignal<{ index: number; x: number; y: number; flipX: number } | null>(null);
  let timer: ReturnType<typeof setTimeout> | undefined;
  onCleanup(() => clearTimeout(timer));
  onMount(() => {
    // Keep the menu on screen.
    const r = el.getBoundingClientRect();
    if (r.bottom > window.innerHeight - 8) el.style.top = `${Math.max(8, window.innerHeight - r.height - 8)}px`;
    // A submenu that does not fit on the right opens to the left of its parent.
    if (r.right > window.innerWidth - 8) el.style.left = `${Math.max(8, props.flipX !== undefined ? props.flipX - r.width : window.innerWidth - r.width - 8)}px`;
    if (props.focus) el.focus();
  });
  const showSub = (index: number, target: HTMLElement) => {
    clearTimeout(timer);
    const r = target.getBoundingClientRect();
    setOpen({ index, x: r.right - 2, y: r.top - 4, flipX: (target.closest(".db-menu")?.getBoundingClientRect().left ?? r.left) + 2 });
  };
  return (
    <div
      class="db-menu"
      classList={{ "db-submenu": props.sub }}
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
    >
      <For each={props.items}>
        {(item, i) => (
          <Show when={!item.separator} fallback={<div class="db-menu-sep" />}>
            <button
              class="db-menu-item"
              classList={{ danger: item.danger, open: open()?.index === i() }}
              role="menuitem"
              aria-haspopup={item.items ? "menu" : undefined}
              disabled={item.disabled}
              onMouseEnter={(e) => {
                const target = e.currentTarget;
                clearTimeout(timer);
                if (item.items) timer = setTimeout(() => showSub(i(), target), 120);
                else timer = setTimeout(() => setOpen(null), 200);
              }}
              onClick={(e) => {
                if (item.items) {
                  showSub(i(), e.currentTarget);
                  return;
                }
                props.onClose();
                item.run?.();
              }}
            >
              <span class="db-menu-icon">{item.icon}</span>
              <span class="db-menu-label">{item.label}</span>
              <Show when={item.hint}>
                <span class="db-menu-hint">{item.hint}</span>
              </Show>
              <Show when={item.items}>
                <ChevronRight size={12} class="db-menu-chevron" />
              </Show>
            </button>
          </Show>
        )}
      </For>
      <Show when={open()}>
        {(o) => <MenuPanel x={o().x} y={o().y} flipX={o().flipX} items={props.items[o().index].items ?? []} onClose={props.onClose} sub />}
      </Show>
    </div>
  );
}
