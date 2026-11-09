/**
 * AI actions in the editor: "Explain with AI" (streamed into the AI Chat), "Fix with AI"
 * for the problems at the caret, "Generate Documentation with AI" and "Generate Tests with
 * AI". Prompts and proposals are built in Rust (`forge-ai-editor`); the proposals open in
 * the diff preview (`ProposalDialog`) and change nothing until the user applies them.
 */
import { createSignal } from "solid-js";
import type { EditorView } from "@codemirror/view";
import { aiEditorApi, type ActionKind, type ActionProblem, type ActionRequest, type Proposal } from "@/ipc/aiEditor";
import { workspaceApi } from "@/ipc/workspace";
import { showBalloon } from "@/keymap";
import { diagnostics } from "@/features/lsp/store";
import { toLsp } from "@/features/lsp/util";
import { openSettings } from "@/features/settings/dialogState";
import { startProviderTurn } from "@/features/ai-chat/store";
import { openFile, reloadFile } from "@/state/editor";
import { showToolWindow } from "@/state/layout";
import { t } from "@/i18n";

export const ACTION_TITLES: Record<ActionKind, string> = {
  explain: "Explain with AI",
  fix: "Fix with AI",
  docs: "Generate Documentation with AI",
  tests: "Generate Tests with AI",
};

/** The diff preview: loading while the model answers, then the proposal or an error. */
export interface ProposalState {
  kind: ActionKind;
  path: string;
  /** The editor the action was started from (its document must not change meanwhile). */
  view: EditorView;
  request: ActionRequest;
  proposal?: Proposal;
  error?: string;
}

const [proposalState, setProposalState] = createSignal<ProposalState | null>(null);
export { proposalState };

let seq = 0;

export function closeProposal(): void {
  seq++;
  const p = proposalState();
  setProposalState(null);
  p?.view.focus();
}

const errorText = (e: unknown) => String(e instanceof Error ? e.message : e);

/** Problems whose range touches the caret line (errors and warnings, not typos). */
export function problemsAt(path: string, view: EditorView): ActionProblem[] {
  const head = view.state.selection.main.head;
  const line = toLsp(view.state.doc, head).line;
  return (diagnostics[path] ?? [])
    .filter((d) => !d.fileLevel && (d.severity === "error" || d.severity === "warning" || d.severity === "weak"))
    .filter((d) => d.range.start.line <= line && line <= d.range.end.line)
    .map((d) => ({ message: d.message, line: d.range.start.line, endLine: d.range.end.line, source: d.source ?? undefined }));
}

export function actionRequest(kind: ActionKind, path: string, view: EditorView, problems?: ActionProblem[]): ActionRequest {
  const sel = view.state.selection.main;
  return { kind, path, text: view.state.doc.toString(), from: sel.from, to: sel.to, problems };
}

/** Errors that mean "set up a provider first" get a link to the settings. */
function failed(title: string, message: string): void {
  const setup = /no AI provider|disabled|API key|no model/i.test(message);
  showBalloon(t("tools.aiEditor.actionFailed", { title }), message, 8000, setup ? [{ label: t("tools.aiEditor.configureAiProviders"), run: () => openSettings("tools.ai-providers") }] : undefined, {
    type: "error",
  });
}

/** Ask for a proposal and show it in the diff preview. */
export async function runProposal(kind: Exclude<ActionKind, "explain">, path: string, view: EditorView, problems?: ActionProblem[]): Promise<void> {
  const request = actionRequest(kind, path, view, problems);
  const id = ++seq;
  setProposalState({ kind, path, view, request });
  try {
    const proposal = await aiEditorApi.action(request);
    if (id === seq) setProposalState({ kind, path, view, request, proposal });
  } catch (e) {
    if (id === seq) setProposalState({ kind, path, view, request, error: errorText(e) });
  }
}

/** Apply the shown proposal: one undoable edit in the editor, or the test file written and opened. */
export async function applyProposal(): Promise<void> {
  const st = proposalState();
  const p = st?.proposal;
  if (!st || !p) return;
  try {
    if (p.edit && p.path === st.path) {
      if (st.view.state.doc.toString() !== st.request.text) {
        setProposalState({ ...st, error: t("tools.aiEditor.fileChangedWhileAnswering") });
        return;
      }
      st.view.dispatch({
        changes: { from: p.edit.from, to: p.edit.to, insert: p.edit.text },
        selection: { anchor: p.edit.from },
        userEvent: "input.ai",
        scrollIntoView: true,
      });
      closeProposal();
      return;
    }
    await workspaceApi.writeFile(p.path, p.proposed);
    closeProposal();
    await reloadFile(p.path).catch(() => undefined);
    await openFile(p.path);
    showBalloon(p.newFile ? t("tools.aiEditor.createdFile", { path: p.path }) : t("tools.aiEditor.updatedFile", { path: p.path }), undefined, 3000);
  } catch (e) {
    setProposalState({ ...st, error: errorText(e) });
  }
}

/** "Explain with AI": the explanation streams into the AI Chat as a new turn. */
export async function explainWithAi(path: string, view: EditorView): Promise<void> {
  const request = actionRequest("explain", path, view);
  type Ev = { delta?: string; done?: boolean; error?: string };
  // Deltas can arrive before the command returns; keep them until the turn exists.
  const early: Ev[] = [];
  let deliver: ((ev: Ev) => void) | null = null;
  try {
    const { started, cancel } = await aiEditorApi.explain(request, (ev) => (deliver ? deliver(ev) : early.push(ev)));
    const lines = view.state.doc.toString().split("\n").slice(started.startLine, started.endLine + 1).join("\n");
    showToolWindow("ai-chat");
    const turn = startProviderTurn(started.title, {
      files: [],
      selection: { path, startLine: started.startLine + 1, endLine: started.endLine + 1, text: lines },
    });
    if (!turn) {
      void cancel();
      showBalloon(t("tools.aiEditor.theAiChatIsBusy"), t("tools.aiEditor.waitForTheAgentTo"), 5000);
      return;
    }
    const note = [started.provider, started.model].filter(Boolean).join(" · ");
    deliver = (ev) => {
      if (ev.delta) turn.append(ev.delta);
      if (ev.error) turn.fail(ev.error);
      else if (ev.done) turn.finish(note);
    };
    for (const ev of early.splice(0)) deliver(ev);
  } catch (e) {
    failed(ACTION_TITLES.explain, errorText(e));
  }
}
