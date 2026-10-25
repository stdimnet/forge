/**
 * PHPStorm's "Trust and Open Project?" dialog (and "Trust Project?" from the Safe Mode
 * banner). A modal <dialog>: Escape answers "Don't Open" / "Cancel", Enter the focused
 * default button; no key listeners needed.
 */
import { createSignal, onMount, Show } from "solid-js";
import { render } from "solid-js/web";
import { ShieldAlert } from "lucide-solid";
import type { TrustCheck } from "@/ipc/trust";

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
      aria-label={open() ? "Trust and Open Project?" : "Trust Project?"}
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
              {open() ? "Trust and Open Project" : "Trust Project"} '{name()}'?
            </h2>
            <p>
              Forge provides features that may execute potentially malicious code contained in this project, for
              example Composer and npm scripts, run configurations, tests, the debugger, build tools and language
              servers installed in the project.
            </p>
            <Show
              when={open()}
              fallback={<p>Trust the project only if it comes from a source you trust. Its scripts and tools can then run.</p>}
            >
              <p>If you don't trust the source, preview the project in Safe Mode: you can browse and edit its files, but nothing of it runs.</p>
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
                    Trust projects in <span class="trust-dialog-mono">{parent()}</span>
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
                Cancel
              </button>
            }
          >
            <button type="button" class="btn-secondary" data-testid="trust-dont-open" onClick={() => answer({ decision: "cancel" })}>
              Don't Open
            </button>
            <span class="trust-dialog-spacer" />
            <button type="button" class="btn-secondary" data-testid="trust-safe-mode" onClick={() => answer({ decision: "safe" })}>
              Preview in Safe Mode
            </button>
          </Show>
          <button type="submit" class="btn-primary" ref={trust} data-testid="trust-project">
            Trust Project
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
