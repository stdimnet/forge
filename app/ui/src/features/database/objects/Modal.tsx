/** The frame of the object dialogs: title, body, footer; Escape closes, the keymap pauses. */
import { onCleanup, onMount, Show, type JSX } from "solid-js";
import { suspendKeymap } from "@/keymap";

export function ObjectModal(props: {
  title: string;
  testId: string;
  width?: number;
  height?: number;
  onClose: () => void;
  footer: JSX.Element;
  /** Left of the footer buttons (an execution mode, a progress bar). */
  footerStart?: JSX.Element;
  children: JSX.Element;
}) {
  let root!: HTMLDivElement;
  const release = suspendKeymap();
  onCleanup(release);
  onMount(() => root.querySelector<HTMLElement>("[data-autofocus], input, select")?.focus());
  return (
    <div class="db-modal-backdrop" onMouseDown={(e) => e.target === e.currentTarget && props.onClose()}>
      <div
        ref={root}
        class="db-modal dbo-modal"
        role="dialog"
        aria-label={props.title}
        data-testid={props.testId}
        style={{ width: `min(${props.width ?? 760}px, calc(100vw - 32px))`, height: props.height ? `min(${props.height}px, calc(100vh - 32px))` : undefined }}
        onKeyDown={(e) => {
          if (e.key === "Escape") {
            e.preventDefault();
            e.stopPropagation();
            props.onClose();
          }
        }}
      >
        <header class="db-modal-title">{props.title}</header>
        <div class="dbo-body">{props.children}</div>
        <footer class="db-modal-actions">
          <Show when={props.footerStart}>{props.footerStart}</Show>
          <span class="db-doc-spacer" />
          {props.footer}
        </footer>
      </div>
    </div>
  );
}

/** "Execute in database" / "Open in console" of the DataGrip dialogs. */
export function ExecutionMode(props: { value: "execute" | "console"; onChange: (v: "execute" | "console") => void; disabled?: boolean }) {
  return (
    <div class="dbo-mode" role="radiogroup" aria-label="Execution">
      <label>
        <input type="radio" name="dbo-mode" checked={props.value === "execute"} disabled={props.disabled} onChange={() => props.onChange("execute")} />
        Execute in database
      </label>
      <label>
        <input type="radio" name="dbo-mode" checked={props.value === "console"} onChange={() => props.onChange("console")} />
        Open in console
      </label>
    </div>
  );
}
