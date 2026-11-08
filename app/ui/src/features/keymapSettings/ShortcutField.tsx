/**
 * A focusable field that records one keystroke (keymap is suspended while focused). Tab and
 * Shift+Tab still move the focus, and Escape keeps its dialog meaning, so the field never
 * traps the keyboard.
 */
import { Show, onCleanup } from "solid-js";
import { X } from "lucide-solid";
import { formatForKeymap, strokeOfEvent, suspendKeymap } from "@/keymap";
import { t } from "@/i18n";

export function ShortcutField(props: {
  value: string;
  onChange: (stroke: string) => void;
  placeholder?: string;
  /** Accessible name (defaults to "Shortcut"). */
  label?: string;
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
      role="textbox"
      aria-label={props.label ?? t("tools.keymapSettings.shortcutField")}
      aria-disabled={props.disabled ? "true" : undefined}
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
        // Escape closes the dialog and Tab / Shift+Tab move the focus: never record them.
        const plain = !e.ctrlKey && !e.altKey && !e.metaKey;
        if ((e.key === "Escape" && plain && !e.shiftKey) || (e.key === "Tab" && plain)) return;
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
