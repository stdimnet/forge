/**
 * The Join Session dialog (Tools | Code With Me | Join Session…, action
 * `CodeWithMe.JoinSession`), as in PHPStorm: paste the host's link, see whom it leads to (the
 * host's name and machine, direct or through a relay) and the name you join with, then
 * **Join**. While the host decides, the dialog shows "Waiting for <host> to accept…" with the
 * verification code to compare and **Cancel**; a declined or unanswered request is shown in
 * the dialog. The link is checked by the backend (`collab_parse_link`); nothing connects
 * before Join. When the project that is closed for the host's has unsaved changes, Join first
 * asks to Save, Don't Save or Cancel, as PHPStorm does before it closes a project.
 *
 * The dialog mounts itself on first use, so it works on the Welcome screen as well.
 */
import { createSignal, For, onCleanup, Show } from "solid-js";
import { render } from "solid-js/web";
import { Users } from "lucide-solid";
import { collabApi, type LinkSummary, type SessionView } from "@/ipc/collab";
import { suspendKeymap } from "@/keymap";
import { openSettings } from "@/features/settings/dialogState";
import { workspace } from "@/state/workspace";
import { editor, isDirty, saveFile, type EditorTab } from "@/state/editor";
import { safeMode } from "@/features/trust/store";
import { t } from "@/i18n";

const LINK = /^(forge:\/\/cwm\/join\?|cwm1:)/;

/** Why `text` is not a session link, or null. */
export function linkProblem(text: string): string | null {
  const link = text.trim();
  if (!link) return t("tools.codeWithMe.pasteLinkFromHost");
  return LINK.test(link) ? null : t("tools.codeWithMe.notALink");
}

/**
 * Files of the open project with unsaved changes, which closing it for the host's project
 * would drop. Shared documents of a session are not the window's own.
 */
export function unsavedFiles(tabs: readonly EditorTab[]): string[] {
  return tabs.filter((tab) => isDirty(tab) && !tab.shared).map((tab) => tab.path);
}

/** The question before the project closes, for `count` unsaved files. */
export function unsavedMessage(project: string, count: number, safe: boolean): string {
  return safe
    ? t("tools.codeWithMe.unsavedChangesSafeMode", { project, count })
    : t("tools.codeWithMe.unsavedChanges", { project, count });
}

/** What the dialog shows. */
export type JoinStep =
  | { step: "link" }
  | { step: "unsaved"; files: string[] }
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
  /** Join, after asking about the unsaved changes of the project it closes. */
  const join = async () => {
    if (!summary() || busy()) return;
    const files = workspace() ? unsavedFiles(editor.tabs) : [];
    if (files.length) return void setStep({ step: "unsaved", files });
    await connect();
  };
  const saveAndJoin = async (files: string[]) => {
    try {
      await Promise.all(files.map((f) => saveFile(f, "explicit")));
    } catch (e) {
      return void setStep({ step: "failed", error: t("tools.codeWithMe.cannotSaveError", { error: message(e) }) });
    }
    await connect();
  };
  const connect = async () => {
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
    else if (unsaved()) setStep({ step: "link" });
    else close(null);
  };
  const unsaved = () => {
    const s = step();
    return s.step === "unsaved" ? s : null;
  };
  const host = () => summary()?.hostName ?? t("tools.codeWithMe.theHost");
  const project = () => workspace()?.name;

  return (
    <div class="popup-backdrop modal">
      <div
        class="dialog cwm-join-dialog"
        style={{ width: "min(500px, calc(100vw - 32px))" }}
        role="dialog"
        aria-label={t("tools.codeWithMe.joinSession")}
        tabIndex={-1}
        onKeyDown={(e) => {
          if (e.key === "Escape") cancel();
          else if (e.key === "Enter" && step().step === "link") void join();
          else if (e.key === "Enter" && unsaved()) void saveAndJoin(unsaved()!.files);
          else return;
          e.preventDefault();
          e.stopPropagation();
        }}
      >
        <div class="dialog-title">{t("tools.codeWithMe.joinSession")}</div>
        <div class="dialog-body">
          <Show when={unsaved()}>
            {(u) => (
              <div class="cwm-join-unsaved" role="alert" data-testid="cwm-join-unsaved">
                <div>{unsavedMessage(project() ?? t("tools.codeWithMe.theProject"), u().files.length, safeMode())}</div>
                <ul class="cwm-join-files">
                  <For each={u().files.slice(0, 5)}>{(f) => <li class="mono">{f}</li>}</For>
                  <Show when={u().files.length > 5}>
                    <li class="muted">{t("tools.codeWithMe.andMore", { count: u().files.length - 5 })}</li>
                  </Show>
                </ul>
              </div>
            )}
          </Show>
          <Show when={!unsaved()}>
            <Show
              when={step().step === "link" || step().step === "failed"}
              fallback={
                <div class="cwm-join-waiting" role="status" aria-live="polite">
                  <Show
                    when={step().step === "waiting" ? (step() as Extract<JoinStep, { step: "waiting" }>) : null}
                    fallback={<div class="cwm-join-headline">{t("tools.codeWithMe.connectingTo", { host: summary()?.hostLabel ?? host() })}</div>}
                  >
                    {(w) => (
                      <>
                        <div class="cwm-join-headline">{t("tools.codeWithMe.waitingForHost", { host: w().host })}</div>
                        <div class="cwm-join-code-label muted">{t("tools.codeWithMe.verificationCode")}</div>
                        <div class="cwm-join-code" data-testid="cwm-join-code">
                          {w().code}
                        </div>
                        <div class="muted cwm-join-hint">{t("tools.codeWithMe.compareCode", { host: w().host })}</div>
                      </>
                    )}
                  </Show>
                </div>
              }
            >
              <label class="form-label" for="cwm-join-link">
                {t("tools.codeWithMe.sessionLinkLabel")}
              </label>
              <input
                id="cwm-join-link"
                ref={(el) => queueMicrotask(() => el.focus())}
                value={link()}
                placeholder={t("tools.codeWithMe.sessionLinkPlaceholder")}
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
                    <dt>{t("tools.codeWithMe.role.host")}</dt>
                    <dd>
                      <Users size={13} /> {s().hostName}
                    </dd>
                    <Show when={s().machine}>
                      <dt>{t("tools.codeWithMe.machine")}</dt>
                      <dd>{s().machine}</dd>
                    </Show>
                    <dt>{t("tools.codeWithMe.connection")}</dt>
                    <dd>{s().relay ? t("tools.codeWithMe.directOrRelay", { relay: s().relay }) : t("tools.codeWithMe.direct")}</dd>
                    <dt>{t("tools.codeWithMe.joinAs")}</dt>
                    <dd>
                      {s().userName}{" "}
                      <button class="link-btn" onClick={() => openSettings("tools.code-with-me")}>
                        {t("tools.codeWithMe.change")}
                      </button>
                    </dd>
                  </dl>
                )}
              </Show>
              <Show when={summary() && project()}>
                <div class="cwm-join-note muted">{t("tools.codeWithMe.projectClosedNote", { project: project(), host: host() })}</div>
              </Show>
              <Show when={step().step === "failed" ? (step() as Extract<JoinStep, { step: "failed" }>) : null}>
                {(f) => (
                  <div class="dialog-error" role="alert">
                    {f().error}
                  </div>
                )}
              </Show>
            </Show>
          </Show>
        </div>
        <div class="dialog-buttons">
          <Show when={unsaved()}>
            <button class="btn-secondary" onClick={() => void connect()}>
              {t("tools.codeWithMe.dontSave")}
            </button>
          </Show>
          <button class="btn-secondary" onClick={cancel}>
            {t("common.cancel")}
          </button>
          <Show when={unsaved()}>
            {(u) => (
              <button class="btn-primary" onClick={() => void saveAndJoin(u().files)}>
                {t("common.save")}
              </button>
            )}
          </Show>
          <Show when={!busy() && !unsaved()}>
            <button class="btn-primary" disabled={!summary()} onClick={() => void join()}>
              {step().step === "failed" ? t("tools.codeWithMe.tryAgain") : t("tools.codeWithMe.join")}
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
