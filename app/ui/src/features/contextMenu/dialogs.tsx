/** Small modal dialogs the context menus need: ask for a name, confirm a destructive action. */
import { createSignal, onCleanup, Show } from "solid-js";
import { suspendKeymap } from "@/keymap";

interface NameRequest {
  kind: "name";
  title: string;
  label: string;
  initial: string;
  confirm: string;
  /** Returns an error message, or null when the name is fine. */
  validate?: (name: string) => string | null;
  done: (name: string | null) => void;
}

interface ConfirmRequest {
  kind: "confirm";
  title: string;
  message: string;
  confirm: string;
  done: (ok: boolean) => void;
}

const [request, setRequest] = createSignal<NameRequest | ConfirmRequest | null>(null);

export function askName(opts: Omit<NameRequest, "kind" | "done">): Promise<string | null> {
  return new Promise((resolve) => setRequest({ kind: "name", ...opts, done: resolve }));
}

export function askConfirm(opts: Omit<ConfirmRequest, "kind" | "done">): Promise<boolean> {
  return new Promise((resolve) => setRequest({ kind: "confirm", ...opts, done: resolve }));
}

function NameDialog(props: { req: NameRequest }) {
  const [value, setValue] = createSignal(props.req.initial);
  const error = () => (value().trim() ? (props.req.validate?.(value().trim()) ?? null) : null);
  const valid = () => value().trim() !== "" && error() === null;
  const close = (result: string | null) => {
    setRequest(null);
    props.req.done(result);
  };
  return (
    <div class="popup-backdrop modal">
      <div
        class="dialog"
        style={{ width: "min(440px, calc(100vw - 32px))" }}
        role="dialog"
        onKeyDown={(e) => {
          if (e.key === "Escape") close(null);
          else if (e.key === "Enter" && valid()) close(value().trim());
          else return;
          e.preventDefault();
          e.stopPropagation();
        }}
      >
        <div class="dialog-title">{props.req.title}</div>
        <div class="dialog-body">
          <label class="form-label">{props.req.label}</label>
          <input
            ref={(el) =>
              queueMicrotask(() => {
                el.focus();
                // Rename selects the base name, not the extension.
                const dot = el.value.lastIndexOf(".");
                el.setSelectionRange(0, dot > 0 ? dot : el.value.length);
              })
            }
            value={value()}
            onInput={(e) => setValue(e.currentTarget.value)}
            spellcheck={false}
          />
          <Show when={error()}>
            <div class="dialog-error">{error()}</div>
          </Show>
        </div>
        <div class="dialog-buttons">
          <button class="btn-secondary" onClick={() => close(null)}>
            Cancel
          </button>
          <button class="btn-primary" disabled={!valid()} onClick={() => close(value().trim())}>
            {props.req.confirm}
          </button>
        </div>
      </div>
    </div>
  );
}

function ConfirmDialog(props: { req: ConfirmRequest }) {
  const close = (ok: boolean) => {
    setRequest(null);
    props.req.done(ok);
  };
  return (
    <div class="popup-backdrop modal">
      <div
        class="dialog"
        style={{ width: "min(440px, calc(100vw - 32px))" }}
        role="alertdialog"
        tabIndex={-1}
        ref={(el) => queueMicrotask(() => el.focus())}
        onKeyDown={(e) => {
          if (e.key === "Escape") close(false);
          else if (e.key === "Enter") close(true);
          else return;
          e.preventDefault();
          e.stopPropagation();
        }}
      >
        <div class="dialog-title">{props.req.title}</div>
        <div class="dialog-body">{props.req.message}</div>
        <div class="dialog-buttons">
          <button class="btn-secondary" onClick={() => close(false)}>
            Cancel
          </button>
          <button class="btn-primary" onClick={() => close(true)}>
            {props.req.confirm}
          </button>
        </div>
      </div>
    </div>
  );
}

export function DialogHost() {
  return (
    <Show when={request()} keyed>
      {(req) => {
        const release = suspendKeymap();
        onCleanup(release);
        return req.kind === "name" ? <NameDialog req={req} /> : <ConfirmDialog req={req} />;
      }}
    </Show>
  );
}
