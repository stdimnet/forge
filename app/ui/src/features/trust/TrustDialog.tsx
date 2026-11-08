/**
 * PHPStorm's "Trust and Open Project?" dialog (and "Trust Project?" from the Safe Mode
 * banner). A modal <dialog>: Escape answers "Don't Open" / "Cancel", Enter the focused
 * default button; no key listeners needed.
 */
import { createSignal, onMount, Show } from "solid-js";
import { render } from "solid-js/web";
import { ShieldAlert } from "lucide-solid";
import type { TrustCheck } from "@/ipc/trust";
import { t } from "@/i18n";

export type TrustAnswer = { decision: "trust"; trustLocation: boolean } | { decision: "safe" } | { decision: "cancel" };

interface Request {
  check: TrustCheck;
  /** `open`: before the project opens (three choices); `trust`: from the Safe Mode banner. */
  mode: "open" | "trust";
  resolve: (answer: TrustAnswer) => void;
}

const [current, setCurrent] = createSignal<Request | null>(null);
let mounted = false;

/** The project's folder name, for the title. */
export function projectName(display: string): string {
  const parts = display.replace(/[\\/]+$/, "").split(/[\\/:]/);
  return parts[parts.length - 1] || display;
}

function Dialog(props: { req: Request }) {
  let dialog!: HTMLDialogElement;
  let trust!: HTMLButtonElement;
  const [location, setLocation] = createSignal(false);
  const answer = (a: TrustAnswer) => {
    setCurrent(null);
    props.req.resolve(a);
  };
  const open = () => props.req.mode === "open";
  const name = () => projectName(props.req.check.display);
  onMount(() => {
    dialog.showModal();
    trust.focus();
  });
  return (
    <dialog
      ref={dialog}
      class="trust-dialog"
      aria-label={open() ? t("tools.trust.trustAndOpenProject") : t("tools.trust.trustProject")}
      data-testid="trust-dialog"
      onCancel={(e) => {
        e.preventDefault();
        answer({ decision: "cancel" });
      }}
    >
      <form method="dialog" onSubmit={(e) => (e.preventDefault(), answer({ decision: "trust", trustLocation: location() }))}>
        <div class="trust-dialog-body">
          <span class="trust-dialog-icon">
            <ShieldAlert size={32} />
          </span>
          <div class="trust-dialog-text">
            <h2 class="trust-dialog-title">
              {open() ? t("tools.trust.trustAndOpenProjectName", { name: name() }) : t("tools.trust.trustProjectName", { name: name() })}
            </h2>
            <p>
              {t("tools.trust.forgeProvidesFeaturesThatMay")}
            </p>
            <Show
              when={open()}
              fallback={<p>{t("tools.trust.trustTheProjectOnlyIf")}</p>}
            >
              <p>{t("tools.trust.ifYouDonTTrust")}</p>
            </Show>
            <code class="trust-dialog-path">{props.req.check.display}</code>
            <Show when={props.req.check.parentDisplay}>
              {(parent) => (
                <label class="trust-dialog-check">
                  <input
                    type="checkbox"
                    checked={location()}
                    onChange={(e) => setLocation(e.currentTarget.checked)}
                    data-testid="trust-location"
                  />
                  <span>
                    {t("tools.trust.trustProjectsIn")} <span class="trust-dialog-mono">{parent()}</span>
                  </span>
                </label>
              )}
            </Show>
          </div>
        </div>
        <footer class="trust-dialog-actions">
          <Show
            when={open()}
            fallback={
              <button type="button" class="btn-secondary" onClick={() => answer({ decision: "cancel" })}>
                {t("common.cancel")}
              </button>
            }
          >
            <button type="button" class="btn-secondary" data-testid="trust-dont-open" onClick={() => answer({ decision: "cancel" })}>
              {t("tools.trust.donTOpen")}
            </button>
            <span class="trust-dialog-spacer" />
            <button type="button" class="btn-secondary" data-testid="trust-safe-mode" onClick={() => answer({ decision: "safe" })}>
              {t("tools.trust.previewInSafeMode")}
            </button>
          </Show>
          <button type="submit" class="btn-primary" ref={trust} data-testid="trust-project">
            {t("tools.trust.trustProject2")}
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

/** Ask whether to trust the project; a dialog that is still open is cancelled first. */
export function askTrust(check: TrustCheck, mode: "open" | "trust"): Promise<TrustAnswer> {
  if (!mounted && typeof document !== "undefined") {
    mounted = true;
    const host = document.createElement("div");
    host.id = "forge-trust-dialogs";
    document.body.appendChild(host);
    render(() => <Host />, host);
  }
  current()?.resolve({ decision: "cancel" });
  return new Promise((resolve) => setCurrent({ check, mode, resolve }));
}
