/**
 * Diff preview of an AI proposal ("Fix with AI", "Generate Documentation / Tests with
 * AI"): a spinner while the model answers, then the change with Apply and Discard. Nothing
 * is changed before Apply.
 */
import { onMount, Show } from "solid-js";
import { Portal } from "solid-js/web";
import { CircleAlert, LoaderCircle, Sparkles, X } from "lucide-solid";
import { FileIcon } from "@/shell/FileIcon";
import { DiffView } from "@/features/ai-chat/DiffView";
import { ACTION_TITLES, applyProposal, closeProposal, proposalState, runProposal } from "./actions";
import { t } from "@/i18n";

export function ProposalDialog() {
  const st = () => proposalState()!;
  const p = () => st().proposal;
  const name = () => (p()?.path ?? st().path).split("/").pop() ?? "";
  let dialog!: HTMLDivElement;
  // Dialog keys while it has focus (Escape discards, Ctrl+Enter applies); the editor's
  // shortcuts stay with the editor.
  const onKey = (e: KeyboardEvent) => {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      closeProposal();
    } else if (e.key === "Enter" && (e.ctrlKey || e.metaKey) && p()) {
      e.preventDefault();
      e.stopPropagation();
      void applyProposal();
    }
  };
  onMount(() => dialog.focus());
  const retry = () => {
    const s = st();
    if (s.kind !== "explain") void runProposal(s.kind, s.path, s.view, s.request.problems);
  };
  return (
    <Portal>
      <div class="chat-dialog-backdrop" onClick={closeProposal}>
        <div
          ref={dialog}
          class="chat-dialog ai-proposal"
          role="dialog"
          tabindex={-1}
          aria-label={ACTION_TITLES[st().kind]}
          onKeyDown={onKey}
          onClick={(e) => e.stopPropagation()}
        >
          <header class="chat-dialog-header">
            <Sparkles size={15} class="ai-proposal-icon" />
            <span class="chat-dialog-title">{p()?.title ?? ACTION_TITLES[st().kind]}</span>
            <span class="chat-flex" />
            <button class="icon-btn" title={t("common.closeEsc")} onClick={closeProposal}>
              <X size={15} />
            </button>
          </header>
          <div class="ai-proposal-file">
            <FileIcon name={name()} size={14} />
            <span class="ai-proposal-name">{name()}</span>
            <span class="muted">{p()?.path ?? st().path}</span>
            <Show when={p()}>
              <span class="ai-proposal-badge">{p()!.newFile ? t("tools.aiEditor.newFile") : "modified"}</span>
              <span class="ai-proposal-stat add">+{p()!.diff.added}</span>
              <span class="ai-proposal-stat del">−{p()!.diff.removed}</span>
            </Show>
          </div>
          <div class="chat-dialog-body ai-proposal-body">
            <Show
              when={p()}
              fallback={
                <Show
                  when={st().error}
                  fallback={
                    <div class="ai-proposal-wait">
                      <LoaderCircle size={16} class="spin" /> {t("tools.aiEditor.waitingForTheAiEllipsis")}
                    </div>
                  }
                >
                  <div class="ai-proposal-error">
                    <CircleAlert size={15} /> {st().error}
                  </div>
                </Show>
              }
            >
              <Show when={st().error}>
                <div class="ai-proposal-error">
                  <CircleAlert size={15} /> {st().error}
                </div>
              </Show>
              <DiffView diff={p()!.diff} path={p()!.path} context={3} />
            </Show>
          </div>
          <footer class="ai-proposal-footer">
            <span class="muted ai-proposal-note">{t("tools.aiEditor.reviewTheChangeBeforeApplying")}</span>
            <span class="chat-flex" />
            <Show when={st().error && !p()}>
              <button class="btn-secondary" onClick={retry}>
                {t("tools.aiEditor.tryAgain")}
              </button>
            </Show>
            <button class="btn-secondary" onClick={closeProposal}>
              {t("tools.aiEditor.discard")}
            </button>
            <button class="btn-primary" disabled={!p()} onClick={() => void applyProposal()} title={t("tools.aiEditor.applyCtrlEnter")}>
              {p()?.newFile ? t("tools.aiEditor.createFile") : t("common.apply")}
            </button>
          </footer>
        </div>
      </div>
    </Portal>
  );
}
