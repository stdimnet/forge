/**
 * A small modal confirmation built on the native <dialog>: focus trapping, Escape to cancel
 * and Enter on the focused default button come from the browser, no key listeners needed.
 */
import { createSignal, For, onMount, Show } from "solid-js";
import { render } from "solid-js/web";
import { CircleHelp, TriangleAlert } from "lucide-solid";
import { t } from "@/i18n";

export interface ConfirmOptions {
  title: string;
  message: string;
  /** Shown in monospace under the message (a path, a config line). */
  detail?: string;
  question?: string;
  okLabel: string;
  cancelLabel?: string;
  warning?: boolean;
}

interface Request extends ConfirmOptions {
  resolve: (ok: boolean) => void;
}

const [current, setCurrent] = createSignal<Request | null>(null);
let mounted = false;

function Dialog(props: { req: Request }) {
  let dialog!: HTMLDialogElement;
  let ok!: HTMLButtonElement;
  const close = (result: boolean) => {
    setCurrent(null);
    props.req.resolve(result);
  };
  onMount(() => {
    dialog.showModal();
    ok.focus();
  });
  return (
    <dialog
      ref={dialog}
      class="bc-confirm"
      aria-label={props.req.title}
      onCancel={(e) => {
        e.preventDefault();
        close(false);
      }}
      onMouseDown={(e) => e.target === dialog && close(false)}
    >
      <form method="dialog" onSubmit={(e) => (e.preventDefault(), close(true))}>
        <header class="bc-confirm-title">{props.req.title}</header>
        <div class="bc-confirm-body">
          <span class="bc-confirm-icon" classList={{ warning: props.req.warning }}>
            <Show when={props.req.warning} fallback={<CircleHelp size={28} />}>
              <TriangleAlert size={28} />
            </Show>
          </span>
          <div class="bc-confirm-text">
            <p>{props.req.message}</p>
            <Show when={props.req.detail}>
              <code class="bc-confirm-detail">
                {/* Allow line breaks after path separators rather than inside names. */}
                <For each={props.req.detail!.split(/(?<=[/\\])/)}>
                  {(part) => (
                    <>
                      {part}
                      <wbr />
                    </>
                  )}
                </For>
              </code>
            </Show>
            <Show when={props.req.question}>
              <p>{props.req.question}</p>
            </Show>
          </div>
        </div>
        <footer class="bc-confirm-actions">
          <button type="button" class="btn-secondary" onClick={() => close(false)}>
            {props.req.cancelLabel ?? t("common.cancel")}
          </button>
          <button type="submit" class="btn-primary" ref={ok}>
            {props.req.okLabel}
          </button>
        </footer>
      </form>
    </dialog>
  );
}

function Host() {
  return (
    <Show when={current()} keyed>
      {(req) => <Dialog req={req} />}
    </Show>
  );
}

/** Ask the user; resolves to true for the OK button. A pending dialog is cancelled first. */
export function confirmDialog(options: ConfirmOptions): Promise<boolean> {
  if (!mounted && typeof document !== "undefined") {
    mounted = true;
    const host = document.createElement("div");
    host.id = "forge-confirm-dialogs";
    document.body.appendChild(host);
    render(() => <Host />, host);
  }
  current()?.resolve(false);
  return new Promise((resolve) => setCurrent({ ...options, resolve }));
}
