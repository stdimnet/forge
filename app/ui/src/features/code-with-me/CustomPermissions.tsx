/**
 * The Custom Permissions dialog: one value per area, as PHPStorm's Code With Me names them.
 * Used for Start Session, the session's Permissions and one guest's Permissions.
 */
import { createSignal, For, onCleanup, Show } from "solid-js";
import { Layer } from "@/shell/layers";
import { suspendKeymap } from "@/keymap";
import type { Permissions } from "@/ipc/collab";
import { t } from "@/i18n";

type Area = keyof Permissions;

/** One value of an area: `label` in the dialog, `short` in summaries ("Read-only"). */
interface AreaValue {
  value: string;
  readonly label: string;
  readonly short: string;
}

const option = (value: string, label: () => string, short: () => string = label): AreaValue => ({
  value,
  get label() {
    return label();
  },
  get short() {
    return short();
  },
});

const noAccess = () => t("tools.codeWithMe.access.none");
const readOnly = () => t("tools.codeWithMe.access.readOnly");
const fullAccess = () => t("tools.codeWithMe.access.full");

export const AREAS: { area: Area; readonly label: string; values: AreaValue[] }[] = [
  {
    area: "files",
    get label() {
      return t("tools.codeWithMe.area.files");
    },
    values: [option("readOnly", readOnly), option("edit", () => t("tools.codeWithMe.access.edit"))],
  },
  {
    area: "terminal",
    get label() {
      return t("tools.codeWithMe.area.terminal");
    },
    values: [option("none", noAccess), option("readOnly", readOnly), option("full", fullAccess)],
  },
  {
    area: "run",
    get label() {
      return t("tools.codeWithMe.area.run");
    },
    values: [
      option("none", noAccess),
      option("readOnly", () => t("tools.codeWithMe.access.run.readOnly"), readOnly),
      option("full", () => t("tools.codeWithMe.access.run.full"), fullAccess),
    ],
  },
  {
    area: "vcs",
    get label() {
      return t("tools.codeWithMe.area.vcs");
    },
    values: [
      option("readOnly", () => t("tools.codeWithMe.access.vcs.readOnly"), readOnly),
      option("full", () => t("tools.codeWithMe.access.vcs.full"), fullAccess),
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
        aria-modal="true"
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
                  {t("tools.codeWithMe.fieldLabel", { label: a.label })}
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
          <div class="cwm-custom-note muted">{t("tools.codeWithMe.protectedFilesNote")}</div>
        </div>
        <div class="dialog-buttons">
          <button class="btn-secondary" onClick={() => close(null)}>
            {t("common.cancel")}
          </button>
          <button class="btn-primary" onClick={() => close(value())}>
            {t("common.ok")}
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
        <Layer kind="modal">
          <Dialog req={req} />
        </Layer>
      )}
    </Show>
  );
}
