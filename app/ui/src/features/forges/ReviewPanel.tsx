/**
 * Review mode panels: the "AI Review" view of a request (Summarize PR, AI Review findings,
 * the provider and model in use, what was not sent) and the Submit Review popup that sends
 * all pending comments as one review.
 */
import { createSignal, For, Show } from "solid-js";
import { CircleCheck, FileCode, ListChecks, LoaderCircle, Send, ShieldAlert, Sparkles, TriangleAlert } from "lucide-solid";
import type { AiInfo, CachedSummary, Finding, FindingsView, ReviewEvent } from "@/ipc/codeReview";
import { openSettings } from "@/features/settings/dialogState";
import { Markdown } from "./Thread";
import { FindingCard, InlineCode, ProviderLine, SeverityIcon } from "./ReviewCards";
import { t as tr } from "@/i18n";

const eventLabels: Record<ReviewEvent, { label: string; hint: string }> = {
  comment: { label: tr("tools.forges.comment"), hint: tr("tools.forges.submitGeneralFeedbackWithoutExplicit") },
  approve: { label: tr("tools.forges.approve"), hint: tr("tools.forges.giveYourApprovalToMerge") },
  requestChanges: { label: tr("tools.forges.requestChanges"), hint: tr("tools.forges.submitFeedbackThatMustBe") },
};

/** The AI provider in use, or why AI is not available. */
export function AiProviderBar(props: { ai: AiInfo }) {
  return (
    <Show
      when={!props.ai.problem}
      fallback={
        <div class="fg-ai-bar fg-ai-bar-problem" data-ai-problem>
          <ShieldAlert size={14} />
          <span>{tr("tools.forges.nothingIsSentToAn", { problem: props.ai.problem })}</span>
          <span class="spacer" />
          <button class="btn-secondary fg-small-btn" onClick={() => openSettings("tools.ai-providers")}>
            {tr("tools.forges.aiProvidersEllipsis")}
          </button>
        </div>
      }
    >
      <div class="fg-ai-bar" data-ai-bar>
        <Sparkles size={14} />
        <span>
          {tr("tools.forges.providerLabel")} <b>{props.ai.provider}</b>
          <Show when={props.ai.model}> · {props.ai.model}</Show>
        </span>
        <span class="fg-ai-where muted fg-small">{props.ai.local ? tr("tools.forges.runsOnThisMachine") : tr("tools.forges.theDiffIsSentTo")}</span>
        <span class="spacer" />
        <button class="btn-secondary fg-small-btn" title={tr("tools.forges.aiAssistantSettings")} onClick={() => openSettings("tools.ai-assistant")}>
          {tr("tools.forges.settingsEllipsis")}
        </button>
      </div>
    </Show>
  );
}

function NotSent(props: { excluded: string[]; omitted: string[] }) {
  return (
    <>
      <Show when={props.excluded.length}>
        <div class="fg-not-sent muted fg-small" data-not-sent>
          <ShieldAlert size={12} />
          {tr("tools.forges.notSentExcludedByAiignore", { excluded: props.excluded.join(", ") })}
        </div>
      </Show>
      <Show when={props.omitted.length}>
        <div class="fg-not-sent muted fg-small">{tr("tools.forges.diffNotSentGeneratedOr", { omitted: props.omitted.join(", ") })}</div>
      </Show>
    </>
  );
}

export function AiReviewView(props: {
  ai: AiInfo;
  headSha: string;
  summary: CachedSummary | null;
  findings: FindingsView | null;
  summarizing: boolean;
  reviewing: boolean;
  summarize: (refresh: boolean) => void;
  review: (refresh: boolean) => void;
  onFinding: (f: Finding, action: "draft" | "dismiss") => void;
  openFile: (path: string) => void;
}) {
  const disabled = () => !!props.ai.problem;
  const s = () => props.summary?.summary;
  return (
    <div class="fg-ai-review" data-ai-review>
      <AiProviderBar ai={props.ai} />
      <div class="fg-ai-actions">
        <button class="btn-primary" disabled={disabled() || props.summarizing} onClick={() => props.summarize(!!props.summary)} data-action="summarize">
          <Show when={props.summarizing} fallback={<Sparkles size={14} />}>
            <LoaderCircle size={14} class="spin" />
          </Show>
          {props.summary ? tr("tools.forges.regenerateSummary") : tr("tools.forges.summarizePr")}
        </button>
        <button class="btn-secondary" disabled={disabled() || props.reviewing} onClick={() => props.review(!!props.findings)} data-action="ai-review">
          <Show when={props.reviewing} fallback={<ListChecks size={14} />}>
            <LoaderCircle size={14} class="spin" />
          </Show>
          {props.findings ? tr("tools.forges.runAiReviewAgain") : tr("tools.forges.aiReview")}
        </button>
        <span class="muted fg-small">{tr("tools.forges.resultsAreKeptForCommit", { headSha: props.headSha.slice(0, 8) })}</span>
      </div>

      <div class="fg-ai-columns">
        <section class="fg-card fg-ai-summary">
          <div class="fg-section-title">{tr("tools.forges.summary")}</div>
          <Show when={s()} fallback={<div class="muted fg-small">{props.summarizing ? tr("tools.forges.summarizingTheChangesEllipsis") : tr("tools.forges.summarizePrWritesAnOverview")}</div>}>
            {(sum) => (
              <>
                <Markdown text={sum().overview} class="fg-ai-overview" />
                <Show when={sum().risks.length}>
                  <div class="fg-ai-sub">{tr("tools.forges.riskAreas")}</div>
                  <For each={sum().risks}>
                    {(r) => (
                      <div class="fg-risk">
                        <SeverityIcon severity={r.severity} size={13} />
                        <div class="fg-risk-body">
                          <InlineCode class="fg-risk-title" text={r.title} />
                          <Show when={r.detail}>
                            <Markdown text={r.detail} class="fg-small" />
                          </Show>
                          <div class="fg-risk-files">
                            <For each={r.files}>
                              {(p) => (
                                <button class="fg-file-chip" onClick={() => props.openFile(p)}>
                                  {p.slice(p.lastIndexOf("/") + 1)}
                                </button>
                              )}
                            </For>
                          </div>
                        </div>
                      </div>
                    )}
                  </For>
                </Show>
                <Show when={sum().files.length}>
                  <div class="fg-ai-sub">{tr("tools.forges.files")}</div>
                  <For each={sum().files}>
                    {(f) => (
                      <div class="fg-ai-file" onClick={() => props.openFile(f.path)}>
                        <FileCode size={13} class="muted" />
                        <span class="fg-ai-file-path">{f.path}</span>
                        <InlineCode class="fg-ai-file-text" text={f.summary} />
                      </div>
                    )}
                  </For>
                </Show>
                <Show when={sum().testPlan.length}>
                  <div class="fg-ai-sub">{tr("tools.forges.suggestedTestPlan")}</div>
                  <ul class="fg-test-plan">
                    <For each={sum().testPlan}>
                      {(t) => (
                        <li>
                          <CircleCheck size={12} class="muted" />
                          <Markdown text={t} />
                        </li>
                      )}
                    </For>
                  </ul>
                </Show>
                <ProviderLine provider={props.summary!.provider} model={props.summary!.model} />
                <NotSent excluded={props.summary!.excluded} omitted={props.summary!.omitted} />
              </>
            )}
          </Show>
        </section>

        <section class="fg-card fg-ai-findings">
          <div class="fg-section-title">
            {tr("tools.forges.findings")}
            <Show when={props.findings}>
              <span class="fg-count">{props.findings!.findings.length}</span>
            </Show>
          </div>
          <Show when={props.findings} fallback={<div class="muted fg-small">{props.reviewing ? tr("tools.forges.reviewingTheChangesEllipsis") : tr("tools.forges.aiReviewProposesInlineComments")}</div>}>
            {(fv) => (
              <>
                <For
                  each={fv().findings}
                  fallback={
                    <div class="muted fg-small">
                      {fv().total ? tr("tools.forges.allFindingsWereAddedTo") : tr("tools.forges.noProblemsFoundInThe")}
                    </div>
                  }
                >
                  {(f) => <FindingCard finding={f} showLocation onAction={props.onFinding} onOpen={(x) => props.openFile(x.path)} />}
                </For>
                <ProviderLine provider={fv().provider} model={fv().model} prefix="Reviewed by" />
                <NotSent excluded={fv().excluded} omitted={fv().omitted} />
              </>
            )}
          </Show>
        </section>
      </div>
    </div>
  );
}

/** "Submit Review" with the number of pending comments; opens the verdict popup. */
export function SubmitReview(props: { pending: number; events: ReviewEvent[]; submit: (event: ReviewEvent, body: string) => Promise<boolean> }) {
  const [open, setOpen] = createSignal(false);
  const [event, setEvent] = createSignal<ReviewEvent>("comment");
  const [body, setBody] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  const send = async () => {
    setBusy(true);
    try {
      if (await props.submit(event(), body())) {
        setBody("");
        setOpen(false);
      }
    } finally {
      setBusy(false);
    }
  };
  return (
    <div class="fg-submit-wrap">
      <button class="btn-primary fg-small-btn" onClick={() => setOpen((o) => !o)} data-action="submit-review">
        <Send size={13} />
        {tr("tools.forges.submitReview")}
        <Show when={props.pending}>
          <span class="fg-count fg-count-on-primary">{props.pending}</span>
        </Show>
      </button>
      <Show when={open()}>
        <div class="popup fg-submit-popup" data-submit-popup>
          <div class="fg-submit-title">{tr("tools.forges.finishYourReview")}</div>
          <textarea rows={4} placeholder={tr("tools.forges.leaveACommentOptionalForEllipsis")} value={body()} onInput={(e) => setBody(e.currentTarget.value)} />
          <For each={props.events}>
            {(ev) => (
              <label class="fg-submit-option">
                <input type="radio" name="review-event" checked={event() === ev} onChange={() => setEvent(ev)} />
                <span>
                  <span class="fg-submit-label">{eventLabels[ev].label}</span>
                  <span class="muted fg-small">{eventLabels[ev].hint}</span>
                </span>
              </label>
            )}
          </For>
          <Show when={!props.events.includes("requestChanges")}>
            <div class="muted fg-small fg-submit-note">
              <TriangleAlert size={11} /> {tr("tools.forges.thisHostHasNoRequest")}
            </div>
          </Show>
          <div class="fg-editor-actions">
            <span class="muted fg-small">
              {tr("tools.forges.pendingComments", { count: props.pending })}
            </span>
            <span class="spacer" />
            <button class="btn-secondary" onClick={() => setOpen(false)}>
              {tr("common.cancel")}
            </button>
            <button class="btn-primary" disabled={busy()} onClick={() => void send()} data-action="submit-review-confirm">
              <Show when={busy()}>
                <LoaderCircle size={13} class="spin" />
              </Show>
              {tr("tools.forges.submitReview")}
            </button>
          </div>
        </div>
      </Show>
    </div>
  );
}
