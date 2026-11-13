/**
 * Typed client for AI in the editor (crates/forge-ai-editor via
 * app/src-tauri/src/commands/ai_editor.rs): inline completion, the editor status, and the
 * Explain / Fix / Generate actions.
 */
import type { EditDiff } from "./agent";
import { invoke, listen } from "./index";
import type { AiStreamEvent } from "./ai";

export interface RelatedFile {
  path: string;
  text: string;
}

export interface InlineRequest {
  /** Project-relative path. */
  path: string;
  language?: string;
  text: string;
  /** Caret (UTF-16 offset). */
  offset: number;
  /** Other open files, most recently used first. */
  related?: RelatedFile[];
}

export interface InlineAnswer {
  /** Ghost text; null when there is nothing to suggest. */
  text: string | null;
  /** Why nothing was asked ("disabled", "cancelled", a policy message). */
  reason?: string;
}

export type Verdict =
  | { status: "allowed" }
  | { status: "projectDisabled" | "languageDisabled" | "ignored"; message: string };

export interface ProviderSummary {
  id: string;
  name: string;
  model?: string | null;
  local: boolean;
  fim: boolean;
}

export interface AiEditorStatus {
  projectEnabled: boolean;
  completionEnabled: boolean;
  completion?: ProviderSummary | null;
  completionProblem?: string | null;
  actions?: ProviderSummary | null;
  actionsProblem?: string | null;
  /** Language id of the asked file (for the per-language switch). */
  language?: string | null;
  /** Inline completion verdict for the asked file. */
  file?: Verdict | null;
  fileActions?: Verdict | null;
}

export type ActionKind = "explain" | "fix" | "docs" | "tests";

export interface ActionProblem {
  message: string;
  /** 0-based line. */
  line: number;
  endLine?: number;
  source?: string;
}

export interface ActionRequest {
  kind: ActionKind;
  path: string;
  language?: string;
  text: string;
  /** Selection (UTF-16); the caret when equal. */
  from: number;
  to: number;
  problems?: ActionProblem[];
}

export interface TextEdit {
  from: number;
  to: number;
  text: string;
}

/** A change proposed by the AI, applied only after the diff preview. */
export interface Proposal {
  kind: ActionKind;
  title: string;
  path: string;
  original: string;
  proposed: string;
  /** Set when `path` is the requesting file. */
  edit?: TextEdit;
  newFile: boolean;
  diff: EditDiff;
}

export interface ExplainStarted {
  title: string;
  prompt: string;
  provider: string;
  model?: string | null;
  /** 0-based first and last line of the explained code. */
  startLine: number;
  endLine: number;
}

let explainCounter = 0;

export const aiEditorApi = {
  /** `temperature` > 0 asks for an alternative suggestion. */
  inlineComplete: (request: InlineRequest, requestId: string, temperature?: number) =>
    invoke<InlineAnswer>("ai_inline_complete", { request, requestId, temperature: temperature ?? null }),
  inlineCancel: (requestId: string) => invoke<void>("ai_inline_cancel", { requestId }),
  status: (path?: string, language?: string) =>
    invoke<AiEditorStatus>("ai_editor_status", { path: path ?? null, language: language ?? null }),
  action: (request: ActionRequest) => invoke<Proposal>("ai_action", { request }),
  /** Stream an explanation; `onEvent` gets the deltas and the final done/error event. */
  explain: async (request: ActionRequest, onEvent: (ev: AiStreamEvent) => void) => {
    const streamId = `x${Date.now().toString(36)}${(explainCounter++).toString(36)}`;
    const unlisten = await listen<AiStreamEvent>(`ai://stream/${streamId}`, (ev) => {
      onEvent(ev);
      if (ev.done || ev.error) unlisten();
    });
    try {
      const started = await invoke<ExplainStarted>("ai_explain", { request, streamId });
      return { streamId, started, cancel: () => invoke<void>("ai_cancel", { streamId }) };
    } catch (e) {
      unlisten();
      throw e;
    }
  },
};
