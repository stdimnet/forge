/**
 * The Custom Permissions dialog: one value per area, as PHPStorm's Code With Me names them.
 * Used for Start Session, the session's Permissions and one guest's Permissions.
 */
import { createSignal, For, onCleanup, Show } from "solid-js";
import { Portal } from "solid-js/web";
import { suspendKeymap } from "@/keymap";
import type { Permissions } from "@/ipc/collab";

type Area = keyof Permissions;

export const AREAS: { area: Area; label: string; values: { value: string; label: string }[] }[] = [
  {
    area: "files",
    label: "Files",
    values: [
      { value: "readOnly", label: "Read-only" },
      { value: "edit", label: "Edit" },
    ],
  },
  {
    area: "terminal",
    label: "Terminal",
    values: [
      { value: "none", label: "No access" },
      { value: "readOnly", label: "Read-only" },
      { value: "full", label: "Full access" },
    ],
  },
  {
    area: "run",
    label: "Run",
    values: [
      { value: "none", label: "No access" },
      { value: "readOnly", label: "Read-only (see output)" },
      { value: "full", label: "Full access (start, stop, rerun)" },
    ],
  },
  {
    area: "vcs",
    label: "Version control",
    values: [
      { value: "readOnly", label: "Read-only (status, diff, blame, log)" },
      { value: "full", label: "Full access (commit, push, checkout, …)" },
    ],
  },
];

interface Request {
  title: string;
  initial: Permissions;
  done: (p: Permissions | null) => void;
}

const [request, setRequest] = createSignal<Request | null>(null);

/** Ask for custom permissions; `null` when cancelled. */
export function askCustomPermissions(title: string, initial: Permissions): Promise<Permissions | null> {
  request()?.done(null);
  return new Promise((done) => setRequest({ title, initial, done }));
}

function Dialog(props: { req: Request }) {
  const [value, setValue] = createSignal<Permissions>({ ...props.req.initial });
  const release = suspendKeymap();
  onCleanup(release);
  const close = (p: Permissions | null) => {
    setRequest(null);
    props.req.done(p);
  };
  return (
    <div class="popup-backdrop modal">
      <div
        class="dialog cwm-custom-dialog"
        style={{ width: "min(460px, calc(100vw - 32px))" }}
        role="dialog"
        aria-label={props.req.title}
        tabIndex={-1}
        ref={(el) => queueMicrotask(() => el.focus())}
        onKeyDown={(e) => {
          if (e.key === "Escape") close(null);
          else if (e.key === "Enter" && !(e.target instanceof HTMLSelectElement)) close(value());
          else return;
          e.preventDefault();
          e.stopPropagation();
        }}
      >
        <div class="dialog-title">{props.req.title}</div>
        <div class="dialog-body cwm-custom-grid">
          <For each={AREAS}>
            {(a) => (
              <>
                <label class="form-label" for={`cwm-custom-${a.area}`}>
                  {a.label}:
                </label>
                <select
                  id={`cwm-custom-${a.area}`}
                  data-area={a.area}
                  value={value()[a.area]}
                  onChange={(e) => setValue((v) => ({ ...v, [a.area]: e.currentTarget.value }) as Permissions)}
                >
                  <For each={a.values}>{(o) => <option value={o.value}>{o.label}</option>}</For>
                </select>
              </>
            )}
          </For>
          <div class="cwm-custom-note muted">
            Changing run configurations, composer.json, package.json and other files that control what runs on your machine needs
            Files Edit and Run Full access.
          </div>
        </div>
        <div class="dialog-buttons">
          <button class="btn-secondary" onClick={() => close(null)}>
            Cancel
          </button>
          <button class="btn-primary" onClick={() => close(value())}>
            OK
          </button>
        </div>
      </div>
    </div>
  );
}

/** Mounted once (with the toolbar button). */
export function CustomPermissionsHost() {
  return (
    <Show when={request()} keyed>
      {(req) => (
        <Portal>
          <Dialog req={req} />
        </Portal>
      )}
    </Show>
  );
}
