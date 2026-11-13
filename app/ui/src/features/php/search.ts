/**
 * Search Everywhere backed by the PHP index: Classes (Ctrl+N) and Symbols (Ctrl+Alt+Shift+N)
 * for PHP projects in the desktop app. Browser mode keeps the demo providers from
 * `mock/searchMock.ts`.
 */
import { isTauri } from "@/ipc";
import { isPluginActive } from "@/state/project";
import { workspace } from "@/state/workspace";
import { registerSearchProvider, type SearchItem, type SearchItemKind } from "@/features/search/providers";
import { phpApi, type SymbolHit, type SymbolKind } from "./api";
import { revealSymbol } from "./editorViews";
import { symbolDedupeKey } from "@/features/langservers/model";

const KINDS: Partial<Record<SymbolKind, SearchItemKind>> = {
  class: "class",
  interface: "interface",
  trait: "trait",
  enum: "enum",
  enumCase: "constant",
  method: "method",
  property: "property",
  constant: "constant",
  function: "function",
};

const MEMBER_KINDS = new Set<SymbolKind>(["method", "property", "constant", "enumCase"]);

/** Map an index hit to a Search Everywhere row. */
export function symbolItem(hit: SymbolHit, projectName = workspace()?.name): SearchItem {
  const member = MEMBER_KINDS.has(hit.kind);
  const label = hit.kind === "property" && !hit.name.startsWith("$") ? `$${hit.name}` : hit.name;
  const shift = label.length - hit.name.length;
  return {
    id: `${hit.path}:${hit.line}:${hit.fqn}`,
    label,
    matches: hit.positions.map((p) => p + shift),
    // Members show their class, class-likes and functions their namespace.
    detail: member ? hit.container.split("\\").pop() : hit.container,
    hint: hit.vendor ? "vendor" : projectName,
    kind: KINDS[hit.kind],
    score: hit.score,
    // Language servers (php-lsp) report the same declarations; this richer item wins.
    dedupeKey: symbolDedupeKey(hit.path, hit.name),
    run: () => revealSymbol(hit.path, { line: hit.line, name: hit.name }),
  };
}

async function search(kind: "class" | "symbol", query: string, includeNonProject: boolean, limit: number): Promise<SearchItem[]> {
  if (!query.trim() || !isPluginActive("forge.php")) return [];
  const hits = await phpApi.searchSymbols(query, kind, includeNonProject ? limit : limit * 2);
  return hits
    .filter((h) => h.kind !== "namespace" && (includeNonProject || !h.vendor))
    .slice(0, limit)
    .map((h) => symbolItem(h));
}

export function registerPhpSearchProviders(): void {
  if (!isTauri()) return;
  registerSearchProvider({
    id: "php.classes",
    tab: "classes",
    title: "Classes",
    order: 0,
    search: (q, o) => search("class", q, o.includeNonProject, o.limit),
  });
  registerSearchProvider({
    id: "php.symbols",
    tab: "symbols",
    title: "Symbols",
    order: 0,
    search: (q, o) => search("symbol", q, o.includeNonProject, o.limit),
  });
}
