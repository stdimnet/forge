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
  /** Label of a checkbox below the message, e.g. "Do not ask me again". */
  checkbox?: string;
  done: (ok: boolean, checked: boolean) => void;
}

interface ChoiceRequest {
  kind: "choice";
  title: string;
  message: string;
  /** Buttons from left to right; the last enabled one is the default (Enter). */
  choices: { id: string; label: string; disabled?: boolean; hint?: string }[];
  done: (id: string | null) => void;
}

const [request, setRequest] = createSignal<NameRequest | ConfirmRequest | ChoiceRequest | null>(null);

export function askName(opts: Omit<NameRequest, "kind" | "done">): Promise<string | null> {
  return new Promise((resolve) => setRequest({ kind: "name", ...opts, done: resolve }));
}

export function askConfirm(opts: Omit<ConfirmRequest, "kind" | "done" | "checkbox">): Promise<boolean> {
  return new Promise((resolve) => setRequest({ kind: "confirm", ...opts, done: (ok) => resolve(ok) }));
}

/** Confirm with a checkbox ("Do not ask me again"); `checked` is its state when confirmed. */
export function askConfirmWithCheckbox(
  opts: Omit<ConfirmRequest, "kind" | "done" | "checkbox"> & { checkbox: string },
): Promise<{ ok: boolean; checked: boolean }> {
  return new Promise((resolve) => setRequest({ kind: "confirm", ...opts, done: (ok, checked) => resolve({ ok, checked }) }));
}

/** Ask the user to pick one of several actions (e.g. Reload / Convert); null when cancelled. */
export function askChoice(opts: Omit<ChoiceRequest, "kind" | "done">): Promise<string | null> {
  return new Promise((resolve) => setRequest({ kind: "choice", ...opts, done: resolve }));
}

function ChoiceDialog(props: { req: ChoiceRequest }) {
  const close = (id: string | null) => {
    setRequest(null);
    props.req.done(id);
  };
  const enabled = () => props.req.choices.filter((c) => !c.disabled);
  return (
    <div class="popup-backdrop modal">
      <div
        class="dialog"
        style={{ width: "min(480px, calc(100vw - 32px))" }}
        role="alertdialog"
        tabIndex={-1}
        ref={(el) => queueMicrotask(() => el.focus())}
        onKeyDown={(e) => {
          if (e.key === "Escape") close(null);
          else if (e.key === "Enter" && enabled().length) close(enabled()[enabled().length - 1].id);
          else return;
          e.preventDefault();
          e.stopPropagation();
        }}
      >
        <div class="dialog-title">{props.req.title}</div>
        <div class="dialog-body">{props.req.message}</div>
        <div class="dialog-buttons">
          <button class="btn-secondary" onClick={() => close(null)}>
            Cancel
          </button>
          {props.req.choices.map((c, i) => (
            <button
              class={i === props.req.choices.length - 1 ? "btn-primary" : "btn-secondary"}
              disabled={c.disabled}
              title={c.hint}
              onClick={() => close(c.id)}
            >
              {c.label}
            </button>
          ))}
        </div>
      </div>
    </div>
  );
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
  const [checked, setChecked] = createSignal(false);
  const close = (ok: boolean) => {
    setRequest(null);
    props.req.done(ok, ok && checked());
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
          <Show when={props.req.checkbox}>
            <label class="dialog-checkbox">
              <input type="checkbox" checked={checked()} onChange={(e) => setChecked(e.currentTarget.checked)} />
              {props.req.checkbox}
            </label>
          </Show>
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
        return req.kind === "name" ? (
          <NameDialog req={req} />
        ) : req.kind === "choice" ? (
          <ChoiceDialog req={req} />
        ) : (
          <ConfirmDialog req={req} />
        );
      }}
    </Show>
  );
}
