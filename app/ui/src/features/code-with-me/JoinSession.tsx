/**
 * The Join Session dialog (Tools | Code With Me | Join Session…, action
 * `CodeWithMe.JoinSession`), as in PHPStorm: paste the host's link, see whom it leads to (the
 * host's name and machine, direct or through a relay) and the name you join with, then
 * **Join**. While the host decides, the dialog shows "Waiting for <host> to accept…" with the
 * verification code to compare and **Cancel**; a declined or unanswered request is shown in
 * the dialog. The link is checked by the backend (`collab_parse_link`); nothing connects
 * before Join.
 *
 * The dialog mounts itself on first use, so it works on the Welcome screen as well.
 */
import { createSignal, onCleanup, Show } from "solid-js";
import { render } from "solid-js/web";
import { Users } from "lucide-solid";
import { collabApi, type LinkSummary, type SessionView } from "@/ipc/collab";
import { suspendKeymap } from "@/keymap";
import { openSettings } from "@/features/settings/dialogState";
import { workspace } from "@/state/workspace";

const LINK = /^(forge:\/\/cwm\/join\?|cwm1:)/;

/** Why `text` is not a session link, or null. */
export function linkProblem(text: string): string | null {
  const t = text.trim();
  if (!t) return "Paste the link from the host.";
  return LINK.test(t) ? null : "This is not a Code With Me link (forge://cwm/join?… or cwm1:…).";
}

/** What the dialog shows. */
export type JoinStep =
  | { step: "link" }
  | { step: "connecting" }
  | { step: "waiting"; host: string; code: string }
  | { step: "failed"; error: string };

interface Request {
  initial: string;
  done: (s: SessionView | null) => void;
}

const [request, setRequest] = createSignal<Request | null>(null);
const [step, setStep] = createSignal<JoinStep>({ step: "link" });

/** Whether the dialog is open (the waiting balloon is not shown then). */
export const joinDialogOpen = () => request() !== null;

/** The guest waits for the host (`collab://waiting`): the dialog shows the code. */
export function showJoinWaiting(host: string, code: string): void {
  if (joinDialogOpen()) setStep({ step: "waiting", host, code });
}

let mounted = false;

/** Open Join Session; resolves with the session once joined, null when cancelled. */
export function openJoinDialog(initial = ""): Promise<SessionView | null> {
  if (!mounted && typeof document !== "undefined") {
    mounted = true;
    const el = document.createElement("div");
    el.className = "cwm-join-host";
    document.body.appendChild(el);
    render(() => <JoinSessionHost />, el);
  }
  request()?.done(null);
  setStep({ step: "link" });
  return new Promise((done) => setRequest({ initial, done }));
}

const message = (e: unknown) => String((e as Error)?.message ?? e);

function Dialog(props: { req: Request }) {
  const [link, setLink] = createSignal(props.req.initial);
  const [summary, setSummary] = createSignal<LinkSummary | null>(null);
  const [problem, setProblem] = createSignal<string | null>(null);
  const release = suspendKeymap();
  onCleanup(release);
  let check = 0;

  const inspect = async (text: string) => {
    const mine = ++check;
    const local = linkProblem(text);
    if (local) {
      setSummary(null);
      setProblem(text.trim() ? local : null);
      return;
    }
    try {
      const s = await collabApi.parseLink(text.trim());
      if (mine !== check) return;
      setSummary(s);
      setProblem(null);
    } catch (e) {
      if (mine !== check) return;
      setSummary(null);
      setProblem(message(e));
    }
  };
  if (props.req.initial) void inspect(props.req.initial);

  const close = (s: SessionView | null) => {
    setRequest(null);
    props.req.done(s);
  };
  const busy = () => step().step === "connecting" || step().step === "waiting";
  const join = async () => {
    if (!summary() || busy()) return;
    setStep({ step: "connecting" });
    try {
      const s = await collabApi.join(link().trim());
      close(s);
    } catch (e) {
      if (message(e) === "Cancelled.") setStep({ step: "link" });
      else setStep({ step: "failed", error: message(e) });
    }
  };
  const cancel = () => {
    if (busy()) void collabApi.cancelJoin().catch(() => undefined);
    else close(null);
  };
  const host = () => summary()?.hostName ?? "the host";
  const project = () => workspace()?.name;

  return (
    <div class="popup-backdrop modal">
      <div
        class="dialog cwm-join-dialog"
        style={{ width: "min(500px, calc(100vw - 32px))" }}
        role="dialog"
        aria-label="Join Session"
        tabIndex={-1}
        onKeyDown={(e) => {
          if (e.key === "Escape") cancel();
          else if (e.key === "Enter" && step().step === "link") void join();
          else return;
          e.preventDefault();
          e.stopPropagation();
        }}
      >
        <div class="dialog-title">Join Session</div>
        <div class="dialog-body">
          <Show
            when={step().step === "link" || step().step === "failed"}
            fallback={
              <div class="cwm-join-waiting" role="status" aria-live="polite">
                <Show
                  when={step().step === "waiting" ? (step() as Extract<JoinStep, { step: "waiting" }>) : null}
                  fallback={<div class="cwm-join-headline">Connecting to {summary()?.hostLabel ?? host()}…</div>}
                >
                  {(w) => (
                    <>
                      <div class="cwm-join-headline">Waiting for {w().host} to accept…</div>
                      <div class="cwm-join-code-label muted">Verification code</div>
                      <div class="cwm-join-code" data-testid="cwm-join-code">
                        {w().code}
                      </div>
                      <div class="muted cwm-join-hint">{w().host} sees the same code. Compare it over a call or chat before they accept.</div>
                    </>
                  )}
                </Show>
              </div>
            }
          >
            <label class="form-label" for="cwm-join-link">
              Session link:
            </label>
            <input
              id="cwm-join-link"
              ref={(el) => queueMicrotask(() => el.focus())}
              value={link()}
              placeholder="forge://cwm/join?…"
              onInput={(e) => {
                setLink(e.currentTarget.value);
                if (step().step === "failed") setStep({ step: "link" });
                void inspect(e.currentTarget.value);
              }}
              spellcheck={false}
            />
            <Show when={problem()}>
              <div class="dialog-error">{problem()}</div>
            </Show>
            <Show when={summary()}>
              {(s) => (
                <dl class="cwm-join-grid" data-testid="cwm-join-summary">
                  <dt>Host</dt>
                  <dd>
                    <Users size={13} /> {s().hostName}
                  </dd>
                  <Show when={s().machine}>
                    <dt>Machine</dt>
                    <dd>{s().machine}</dd>
                  </Show>
                  <dt>Connection</dt>
                  <dd>{s().relay ? `Direct, or through the relay server ${s().relay}` : "Direct"}</dd>
                  <dt>Join as</dt>
                  <dd>
                    {s().userName}{" "}
                    <button class="link-btn" onClick={() => openSettings("tools.code-with-me")}>
                      Change…
                    </button>
                  </dd>
                </dl>
              )}
            </Show>
            <Show when={summary() && project()}>
              <div class="cwm-join-note muted">The project {project()} is closed while you are a guest of {host()}.</div>
            </Show>
            <Show when={step().step === "failed" ? (step() as Extract<JoinStep, { step: "failed" }>) : null}>
              {(f) => (
                <div class="dialog-error" role="alert">
                  {f().error}
                </div>
              )}
            </Show>
          </Show>
        </div>
        <div class="dialog-buttons">
          <button class="btn-secondary" onClick={cancel}>
            Cancel
          </button>
          <Show when={!busy()}>
            <button class="btn-primary" disabled={!summary()} onClick={() => void join()}>
              {step().step === "failed" ? "Try Again" : "Join"}
            </button>
          </Show>
        </div>
      </div>
    </div>
  );
}

function JoinSessionHost() {
  return <Show when={request()} keyed>{(req) => <Dialog req={req} />}</Show>;
}
