/** A dropdown menu anchored below a button (filters, merge options). */
import { For, onCleanup, onMount, Show, type JSX } from "solid-js";
import { Check } from "lucide-solid";

export interface MenuItem {
  label: string;
  hint?: string;
  checked?: boolean;
  icon?: JSX.Element;
  separator?: boolean;
  run?: () => void;
}

export function Menu(props: { anchor: HTMLElement; items: MenuItem[]; onClose: () => void; width?: number }) {
  let ref!: HTMLDivElement;
  const rect = props.anchor.getBoundingClientRect();
  const left = Math.min(rect.left, window.innerWidth - (props.width ?? 220) - 8);

  const onDown = (e: MouseEvent) => {
    if (!ref.contains(e.target as Node) && !props.anchor.contains(e.target as Node)) props.onClose();
  };
  onMount(() => {
    document.addEventListener("mousedown", onDown, true);
    ref.focus();
  });
  onCleanup(() => document.removeEventListener("mousedown", onDown, true));

  return (
    <div
      ref={ref}
      class="popup fg-menu"
      tabindex="-1"
      style={{ left: `${left}px`, top: `${rect.bottom + 4}px`, "min-width": `${props.width ?? 220}px` }}
      onKeyDown={(e) => {
        if (e.key === "Escape") {
          e.stopPropagation();
          props.onClose();
        }
      }}
    >
      <For each={props.items}>
        {(item) => (
          <Show when={!item.separator} fallback={<div class="popup-sep" />}>
            <div
              class="popup-row"
              role="menuitem"
              onClick={() => {
                props.onClose();
                item.run?.();
              }}
            >
              <span class="popup-icon">{item.checked ? <Check size={14} /> : item.icon}</span>
              <span class="popup-label">{item.label}</span>
              <Show when={item.hint}>
                <span class="popup-hint">{item.hint}</span>
              </Show>
            </div>
          </Show>
        )}
      </For>
    </div>
  );
}
