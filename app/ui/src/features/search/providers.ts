/**
 * Search Everywhere contribution point. Each provider serves one tab (Classes, Files,
 * Symbols, Actions, Text); the "All" tab merges them. Language packs plug in real
 * class/symbol indexes with:
 *
 *   registerSearchProvider({ id: "php.classes", tab: "classes", title: "Classes", search })
 *
 * A provider registered with an existing id replaces it (e.g. the PHP pack replacing the
 * demo-project mock).
 */
import type { JSX } from "solid-js";
import { createSignal } from "solid-js";

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
  /** Highlighted character indices into `label`. */
  matches?: number[];
  /** Grey text right after the label (namespace, containing folder, line preview). */
  detail?: string;
  /** Right-aligned hint (shortcut for actions, module/location otherwise). */
  hint?: string;
  kind?: SearchItemKind;
  /** Custom icon; defaults to an icon derived from `kind`. */
  icon?: () => JSX.Element;
  /** Higher is better; used to merge providers within a tab. */
  score: number;
  /** Shown dimmed (e.g. actions that are not implemented yet). */
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
  setSearchProviders((prev) =>
    [...prev.filter((x) => x.id !== p.id), p].sort((a, b) => (a.order ?? 100) - (b.order ?? 100)),
  );
}

export { searchProviders };

export const providersFor = (tab: "all" | SearchTab): SearchProvider[] => {
  const order = SEARCH_TABS.map((t) => t.id);
  const list = searchProviders().filter((p) => tab === "all" || p.tab === tab);
  return list.sort((a, b) => order.indexOf(a.tab) - order.indexOf(b.tab) || (a.order ?? 100) - (b.order ?? 100));
};

export interface SearchGroup {
  provider: SearchProvider;
  items: SearchItem[];
  /** More results exist than shown (the "All" tab shows a few per group). */
  more: boolean;
}

/** Query all providers of a tab. Failing providers are logged and skipped. */
export async function runSearch(
  tab: "all" | SearchTab,
  query: string,
  opts: { includeNonProject: boolean; perGroup: number },
): Promise<SearchGroup[]> {
  const providers = providersFor(tab);
  const limit = opts.perGroup + 1;
  const results = await Promise.all(
    providers.map(async (provider) => {
      try {
        const items = await provider.search(query, { includeNonProject: opts.includeNonProject, limit });
        return { provider, items };
      } catch (err) {
        console.warn(`search provider ${provider.id} failed`, err);
        return { provider, items: [] as SearchItem[] };
      }
    }),
  );
  // Providers serving the same tab are merged into one group (e.g. PHP + JS classes).
  const byTab = new Map<SearchTab, SearchGroup>();
  for (const { provider, items } of results) {
    const g = byTab.get(provider.tab);
    if (g) g.items.push(...items);
    else byTab.set(provider.tab, { provider, items: [...items], more: false });
  }
  // In "All", an item already listed by an earlier group (e.g. a class that is also a
  // symbol) is not repeated.
  const seen = new Set<string>();
  return [...byTab.values()]
    .map((g) => {
      if (tab === "all") {
        g.items = g.items.filter((i) => {
          const key = `${i.kind ?? ""}:${i.id}`;
          if (seen.has(key)) return false;
          seen.add(key);
          return true;
        });
      }
      const sorted = g.items.sort((a, b) => b.score - a.score);
      return { ...g, items: sorted.slice(0, opts.perGroup), more: sorted.length > opts.perGroup };
    })
    .filter((g) => g.items.length > 0);
}
