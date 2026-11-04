/** A focusable field that records one keystroke (keymap is suspended while focused). */
import { Show, onCleanup } from "solid-js";
import { X } from "lucide-solid";
import { formatForKeymap, strokeOfEvent, suspendKeymap } from "@/keymap";
import { t } from "@/i18n";

export function ShortcutField(props: {
  value: string;
  onChange: (stroke: string) => void;
  placeholder?: string;
  autofocus?: boolean;
  disabled?: boolean;
  ref?: (el: HTMLDivElement) => void;
}) {
  let resume: (() => void) | undefined;
  const release = () => {
    resume?.();
    resume = undefined;
  };
  onCleanup(release);

  return (
    <div
      class="shortcut-field"
      classList={{ disabled: props.disabled, empty: !props.value }}
      tabIndex={props.disabled ? -1 : 0}
      ref={(el) => {
        props.ref?.(el);
        if (props.autofocus) queueMicrotask(() => el.focus());
      }}
      onFocus={() => {
        release();
        resume = suspendKeymap();
      }}
      onBlur={release}
      onKeyDown={(e) => {
        // Escape/Tab keep their dialog meaning when the field is empty.
        if ((e.key === "Escape" || e.key === "Tab") && !e.ctrlKey && !e.altKey && !e.metaKey && !e.shiftKey) return;
        e.preventDefault();
        e.stopPropagation();
        const stroke = strokeOfEvent(e);
        if (stroke) props.onChange(stroke);
      }}
    >
      <span class="shortcut-field-text">{props.value ? formatForKeymap(props.value) : (props.placeholder ?? t("tools.keymapSettings.pressKeysEllipsis"))}</span>
      <Show when={props.value && !props.disabled}>
        <button
          class="icon-btn shortcut-field-clear"
          title={t("common.clear")}
          tabIndex={-1}
          onMouseDown={(e) => e.preventDefault()}
          onClick={() => props.onChange("")}
        >
          <X size={12} />
        </button>
      </Show>
    </div>
  );
}
