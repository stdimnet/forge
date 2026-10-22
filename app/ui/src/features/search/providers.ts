/**
 * Search Everywhere contribution point. Each provider serves one tab (Classes, Files,
 * Symbols, Actions, Text); the "All" tab merges them. Language packs plug in real
 * class/symbol indexes with:
 *
 *   registerSearchProvider({ id: "php.classes", tab: "classes", title: "Classes", search })
 *
 * A provider registered with an existing id replaces it (e.g. the PHP pack replacing the
 * demo-project mock).
 *
 * Providers only *collect* candidates; matching, scoring, highlighting, merging, deduping
 * and cutting happen in Rust (`forge_fuzzy::everywhere`, the `search_rank` command). A
 * candidate is either pre-ranked by a Rust backend (`score` + `matches`, e.g. project files
 * or the PHP index) or carries `match` to be matched against the query there.
 */
import type { JSX } from "solid-js";
import { createSignal } from "solid-js";
import { exactMatching, searchApi, type RankCandidate } from "@/ipc/search";

export type SearchTab = "classes" | "files" | "symbols" | "actions" | "text";

export const SEARCH_TABS: { id: "all" | SearchTab; title: string }[] = [
  { id: "all", title: "All" },
  { id: "classes", title: "Classes" },
  { id: "files", title: "Files" },
  { id: "symbols", title: "Symbols" },
  { id: "actions", title: "Actions" },
  { id: "text", title: "Text" },
];

export type SearchItemKind =
  | "class"
  | "interface"
  | "trait"
  | "enum"
  | "function"
  | "method"
  | "property"
  | "constant"
  | "file"
  | "action"
  | "text";

export interface SearchItem {
  /** Unique within its provider. */
  id: string;
  label: string;
  /** Highlighted character indices into `label` (pre-ranked items; set by Rust otherwise). */
  matches?: number[];
  /** Grey text right after the label (namespace, containing folder, line preview). */
  detail?: string;
  /** Right-aligned hint (shortcut for actions, module/location otherwise). */
  hint?: string;
  kind?: SearchItemKind;
  /** Custom icon; defaults to an icon derived from `kind`. */
  icon?: () => JSX.Element;
  /** Pre-ranked items: their score. Items with `match`: a bonus on top of the match score. */
  score?: number;
  /** Match the query against `label` in Rust. */
  match?: {
    /** Word-start matching, as PHPStorm does for actions, classes and symbols. */
    strict?: boolean;
    /** Also matched (ranked lower, without highlighting), e.g. an action id. */
    altTexts?: string[];
    /** Keep the item when the label does not match (the source already filtered it). */
    keepUnmatched?: boolean;
  };
  /** Shown dimmed (e.g. actions that are not available); hidden while others match. */
  dimmed?: boolean;
  run: () => void | Promise<void>;
}

export interface SearchOptions {
  includeNonProject: boolean;
  limit: number;
}

export interface SearchProvider {
  id: string;
  tab: SearchTab;
  /** Group title in the "All" tab. */
  title: string;
  order?: number;
  search(query: string, opts: SearchOptions): SearchItem[] | Promise<SearchItem[]>;
}

const [searchProviders, setSearchProviders] = createSignal<SearchProvider[]>([]);

export function registerSearchProvider(p: SearchProvider): void {
  setSearchProviders((prev) => [...prev.filter((x) => x.id !== p.id), p]);
}

export { searchProviders };

export interface SearchGroup {
  provider: SearchProvider;
  items: SearchItem[];
  /** More results exist than shown (the "All" tab shows a few per group). */
  more: boolean;
}

function candidate(item: SearchItem): RankCandidate {
  const base = { id: item.id, kind: item.kind, score: item.score ?? 0, dimmed: item.dimmed };
  if (!item.match) return { ...base, positions: item.matches ?? [] };
  return { ...base, text: item.label, ...item.match };
}

interface ProviderResult {
  provider: SearchProvider;
  items: SearchItem[];
}

type RunOptions = { includeNonProject: boolean; perGroup: number };

async function queryProvider(provider: SearchProvider, query: string, opts: RunOptions): Promise<ProviderResult> {
  // Providers that query a backend fetch a bit more than a page, so "… more" is known.
  const limit = opts.perGroup + 1;
  try {
    return { provider, items: await provider.search(query, { includeNonProject: opts.includeNonProject, limit }) };
  } catch (err) {
    console.warn(`search provider ${provider.id} failed`, err);
    return { provider, items: [] };
  }
}

/** Query all providers of a tab and rank their items in Rust. Failing providers are skipped. */
export async function runSearch(tab: "all" | SearchTab, query: string, opts: RunOptions): Promise<SearchGroup[]> {
  const providers = searchProviders().filter((p) => tab === "all" || p.tab === tab);
  return rankResults(tab, query, opts, await Promise.all(providers.map((p) => queryProvider(p, query, opts))));
}

/**
 * [`runSearch`] that reports the ranked groups every time another provider has answered, so a
 * fast provider's results show while a slow one is still working. `done` is true on the last
 * update. Updates arrive in the order the providers finished.
 */
export async function runSearchStreaming(
  tab: "all" | SearchTab,
  query: string,
  opts: RunOptions,
  onUpdate: (groups: SearchGroup[], done: boolean) => void,
): Promise<void> {
  const providers = searchProviders().filter((p) => tab === "all" || p.tab === tab);
  if (!providers.length) {
    onUpdate([], true);
    return;
  }
  const answered: ProviderResult[] = [];
  let reported = 0;
  await Promise.all(
    providers.map(async (provider) => {
      answered.push(await queryProvider(provider, query, opts));
      const snapshot = answered.slice();
      const mine = snapshot.length;
      const groups = await rankResults(tab, query, opts, snapshot);
      if (mine > reported) {
        reported = mine;
        onUpdate(groups, mine === providers.length);
      }
    }),
  );
}

async function rankResults(
  tab: "all" | SearchTab,
  query: string,
  opts: RunOptions,
  results: ProviderResult[],
): Promise<SearchGroup[]> {
  const { groups } = await searchApi.rank({
    tab,
    query,
    perGroup: opts.perGroup,
    includeDimmed: opts.includeNonProject,
    exact: exactMatching(),
    sources: results.map(({ provider, items }) => ({
      provider: provider.id,
      tab: provider.tab,
      order: provider.order,
      items: items.map(candidate),
    })),
  });
  const byId = new Map(results.map((r) => [r.provider.id, r]));
  return groups.map((g) => ({
    provider: byId.get(g.provider)!.provider,
    more: g.more,
    items: g.items.map((hit) => {
      const item = byId.get(hit.provider)!.items[hit.index];
      return { ...item, score: hit.score, matches: hit.positions.length ? hit.positions : item.matches };
    }),
  }));
}
