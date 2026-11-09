/**
 * The modal frame of the Git dialogs (title, body, footer with Cancel and the main button)
 * and their form rows. Keys inside a dialog are handled on the dialog itself while the keymap
 * is suspended, like the other git modals.
 */
import { onCleanup, onMount, type JSX } from "solid-js";
import { suspendKeymap } from "@/keymap";
import { closeGitDialog } from "./gitDialogs";
import { t } from "@/i18n";

/**
 * Whether Enter confirms a dialog: not on a button (Enter presses the focused button, so
 * Cancel cancels), not on a checkbox or radio button, and in a text area only with Ctrl/Cmd.
 */
export function enterConfirms(e: KeyboardEvent): boolean {
  if (e.key !== "Enter") return false;
  const t = e.target as HTMLElement | null;
  if (!t) return true;
  if (t.tagName === "BUTTON") return false;
  if (t.tagName === "INPUT" && ["checkbox", "radio"].includes((t as HTMLInputElement).type)) return false;
  return t.tagName !== "TEXTAREA" || e.ctrlKey || e.metaKey;
}

/** Modal frame: title, body, footer; Escape cancels, Enter (see `enterConfirms`) confirms. */
export function Frame(props: {
  title: string;
  class?: string;
  children: JSX.Element;
  okLabel: string;
  okDisabled?: boolean;
  onOk: () => void | Promise<void>;
  /** Buttons left of Cancel. */
  extra?: JSX.Element;
}) {
  let root!: HTMLDivElement;
  const release = suspendKeymap();
  onCleanup(release);
  onMount(() => root.querySelector<HTMLElement>("[data-autofocus], input:not([type=checkbox]):not([type=radio]), textarea, select")?.focus());
  const ok = () => {
    if (!props.okDisabled) void props.onOk();
  };
  const onKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Escape") closeGitDialog();
    else if (enterConfirms(e)) ok();
    else return;
    e.preventDefault();
    e.stopPropagation();
  };
  return (
    <div class="modal-backdrop" onMouseDown={(e) => e.target === e.currentTarget && closeGitDialog()}>
      <div ref={root} class={`git-dialog gd ${props.class ?? ""}`} role="dialog" aria-label={props.title} onKeyDown={onKeyDown}>
        <header class="dialog-title">
          <span>{props.title}</span>
        </header>
        <div class="git-dialog-body gd-body">{props.children}</div>
        <footer class="git-dialog-actions">
          {props.extra}
          <span class="spacer" />
          <button class="btn-secondary" onClick={closeGitDialog}>
            {t("common.cancel")}
          </button>
          <button class="btn-primary" disabled={props.okDisabled} onClick={ok}>
            {props.okLabel}
          </button>
        </footer>
      </div>
    </div>
  );
}

export function Row(props: { label: string; children: JSX.Element }) {
  return (
    <>
      <span class="gd-label">{props.label}</span>
      <div class="gd-value">{props.children}</div>
    </>
  );
}

export function Check(props: { label: string; checked: boolean; onChange: (on: boolean) => void; disabled?: boolean; hint?: string }) {
  return (
    <label class="git-dialog-check" classList={{ disabled: props.disabled }} title={props.hint}>
      <input type="checkbox" checked={props.checked} disabled={props.disabled} onChange={(e) => props.onChange(e.currentTarget.checked)} />
      {props.label}
    </label>
  );
}

