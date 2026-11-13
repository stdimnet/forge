import { createSignal, For, onCleanup, Show, type JSX } from "solid-js";
import { Check, ChevronDown } from "lucide-solid";
import { t } from "@/i18n";

export interface DropdownItem<T extends string | number> {
  value: T;
  label: string;
  hint?: string;
  icon?: JSX.Element;
  badge?: string;
  disabled?: boolean;
}

/** Compact popup picker in the PHPStorm New UI style (toolbar combo). */
export function Dropdown<T extends string | number>(props: {
  items: DropdownItem<T>[];
  value?: T;
  onSelect: (v: T) => void;
  trigger: JSX.Element;
  title?: string;
  class?: string;
  align?: "left" | "right";
  header?: string;
  empty?: string;
  hideChevron?: boolean;
}) {
  const [open, setOpen] = createSignal(false);
  let root!: HTMLDivElement;
  const onDocDown = (e: MouseEvent) => {
    if (!root.contains(e.target as Node)) setOpen(false);
  };
  document.addEventListener("mousedown", onDocDown);
  onCleanup(() => document.removeEventListener("mousedown", onDocDown));

  return (
    <div class={`chat-dropdown ${props.class ?? ""}`} ref={root}>
      <button
        class="chat-dropdown-trigger"
        classList={{ open: open() }}
        title={props.title}
        aria-haspopup="listbox"
        aria-expanded={open()}
        onClick={() => setOpen(!open())}
      >
        {props.trigger}
        <Show when={!props.hideChevron}>
          <ChevronDown size={12} class="chat-dropdown-chevron" />
        </Show>
      </button>
      <Show when={open()}>
        <div class="chat-popup chat-dropdown-menu" classList={{ right: props.align === "right" }} role="listbox">
          <Show when={props.header}>
            <div class="chat-popup-title">{props.header}</div>
          </Show>
          <For each={props.items} fallback={<div class="chat-popup-empty">{props.empty ?? t("tools.aiChat.nothingHere")}</div>}>
            {(item) => (
              <button
                class="chat-popup-item"
                classList={{ selected: item.value === props.value }}
                role="option"
                aria-selected={item.value === props.value}
                disabled={item.disabled}
                onClick={() => {
                  setOpen(false);
                  props.onSelect(item.value);
                }}
              >
                <span class="chat-popup-icon">{item.icon}</span>
                <span class="chat-popup-text">
                  <span class="chat-popup-label">{item.label}</span>
                  <Show when={item.hint}>
                    <span class="chat-popup-hint block">{item.hint}</span>
                  </Show>
                </span>
                <Show when={item.badge}>
                  <span class="chat-badge">{item.badge}</span>
                </Show>
                <Show when={item.value === props.value}>
                  <Check size={13} class="chat-popup-check" />
                </Show>
              </button>
            )}
          </For>
        </div>
      </Show>
    </div>
  );
}
