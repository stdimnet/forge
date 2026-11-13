/**
 * Review mode of pull/merge requests: typed client for `commands/code_review.rs`
 * (`forge-code-review`). Viewed files, pending comments and AI results live in Forge's
 * cache folder; nothing is posted to the code host before `submit`.
 */
import { invoke } from "./index";
import type { ChangedFile, Position } from "./forges";

export type ReviewEvent = "comment" | "approve" | "requestChanges";
export type Severity = "error" | "warning" | "info";

export interface ReviewFile {
  path: string;
  /** Digest of the file's diff; a new push changes it and the file is unviewed again. */
  digest: string;
  viewed: boolean;
  /** Excluded from AI by `.aiignore` (or AI is off for the project). */
  aiExcluded: boolean;
}

export interface Draft {
  /** Empty for a new draft. */
  id: string;
  body: string;
  /** `null`: a general comment. */
  position: Position | null;
  startLine?: number | null;
  /** The AI finding it was made from. */
  finding?: string | null;
}

export interface Risk {
  title: string;
  detail: string;
  severity: Severity;
  files: string[];
}

export interface Summary {
  overview: string;
  risks: Risk[];
  files: { path: string; summary: string }[];
  testPlan: string[];
}

export interface CachedSummary {
  headSha: string;
  summary: Summary;
  provider: string;
  model: string | null;
  excluded: string[];
  omitted: string[];
  createdAt: number;
}

export interface Finding {
  id: string;
  path: string;
  line: number;
  endLine: number | null;
  severity: Severity;
  title: string;
  explanation: string;
  suggestion: string | null;
  /** Where an inline comment goes; `null` when the line is not in the diff. */
  position: Position | null;
  startLine: number | null;
}

export interface FindingsView {
  headSha: string;
  findings: Finding[];
  total: number;
  provider: string;
  model: string | null;
  excluded: string[];
  omitted: string[];
}

export interface AiInfo {
  provider: string | null;
  model: string | null;
  local: boolean;
  /** Why AI cannot be used (no provider, AI off for the project, …). */
  problem: string | null;
}

export interface ReviewSession {
  headSha: string;
  files: ReviewFile[];
  changes: ChangedFile[];
  drafts: Draft[];
  events: ReviewEvent[];
  summary: CachedSummary | null;
  findings: FindingsView | null;
  ai: AiInfo;
}

export interface FindingUpdate {
  drafts: Draft[];
  findings: FindingsView | null;
}

export interface ExplainResult {
  text: string;
  provider: string;
  model: string | null;
}

export const reviewApi = {
  session: (number: number) => invoke<ReviewSession>("review_session", { number }),
  setViewed: (number: number, path: string, digest: string, viewed: boolean) =>
    invoke<void>("review_set_viewed", { number, path, digest, viewed }),
  saveDraft: (number: number, draft: Draft) => invoke<Draft[]>("review_save_draft", { number, draft }),
  deleteDraft: (number: number, id: string) => invoke<Draft[]>("review_delete_draft", { number, id }),
  submit: (number: number, event: ReviewEvent, body: string) => invoke<void>("review_submit", { number, event, body }),
  summarize: (number: number, refresh = false) => invoke<CachedSummary>("review_summarize", { number, refresh }),
  aiReview: (number: number, refresh = false) => invoke<FindingsView>("review_ai", { number, refresh }),
  finding: (number: number, id: string, action: "draft" | "dismiss") =>
    invoke<FindingUpdate>("review_finding", { number, id, action }),
  explain: (number: number, path: string, hunk: number) => invoke<ExplainResult>("review_explain", { number, path, hunk }),
};
