/**
 * Search IPC: "Go to File", fuzzy filtering of in-memory lists and Search Everywhere
 * ranking. Mirrors `crates/forge-fuzzy` and `app/src-tauri/src/commands/{workspace,search}.rs`;
 * browser mode runs the same Rust code as WebAssembly.
 */
import { invoke } from "./index";

export interface FileMatch {
  path: string;
  name: string;
  score: number;
  /** Highlight positions into `name`, or into `path` when `matchedPath`. */
  positions: number[];
  matchedPath: boolean;
}

/** One text, or a name with aliases (the first that matches counts). */
export type FilterCandidate = string | string[];

export interface FilterRequest {
  query: string;
  candidates: FilterCandidate[];
  /** Word-start matching (actions, classes, symbols). */
  strict?: boolean;
  /** On equal scores prefer shorter texts (otherwise the input order is kept). */
  preferShorter?: boolean;
  limit?: number;
}

export interface FilterHit {
  /** Index into `candidates`. */
  index: number;
  /** Which text of the candidate matched. */
  text: number;
  score: number;
  /** Highlight positions (char indices) into the matched text. */
  positions: number[];
}

/** A Search Everywhere candidate as collected by a provider (see `forge_fuzzy::everywhere`). */
export interface RankCandidate {
  id: string;
  kind?: string;
  /** Matched against the query here in Rust; omit for pre-ranked items. */
  text?: string;
  /** Matched when `text` does not (e.g. an action id); ranked lower, not highlighted. */
  altTexts?: string[];
  strict?: boolean;
  /** Keep the item even when the query does not match `text`. */
  keepUnmatched?: boolean;
  /** Pre-computed score, or a bonus on top of the match score for text candidates. */
  score?: number;
  /** Pre-computed highlight positions of pre-ranked items. */
  positions?: number[];
  dimmed?: boolean;
}

export interface RankRequest {
  tab: string;
  query: string;
  perGroup: number;
  includeDimmed?: boolean;
  sources: { provider: string; tab: string; order?: number; items: RankCandidate[] }[];
}

export interface RankedItem {
  provider: string;
  /** Index into the provider's items. */
  index: number;
  score: number;
  positions: number[];
}

export interface RankResponse {
  groups: { tab: string; provider: string; items: RankedItem[]; more: boolean }[];
}

export const searchApi = {
  /** Fuzzy "Go to File" over the project (respects .gitignore). */
  findFiles: (query: string, limit = 100) => invoke<FileMatch[]>("fs_find_files", { query, limit }),
  /** Keep and rank the candidates matching `query`, best first. */
  filter: (request: FilterRequest) => invoke<FilterHit[]>("fuzzy_filter", { request }),
  /** Merge, score, dedupe and group Search Everywhere candidates. */
  rank: (request: RankRequest) => invoke<RankResponse>("search_rank", { request }),
};
