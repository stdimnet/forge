/**
 * Search Everywhere over every running language server (`workspace/symbol`): the Symbols
 * tab gets all symbols, the Classes tab classes, interfaces, structs and enums. This gives
 * project-wide symbol search for TypeScript, Python, Go, Rust, CSS, YAML, ... alike.
 */
import { langserversApi, type WorkspaceSymbol } from "@/ipc/langservers";
import { fuzzyMatch } from "@/features/search/fuzzy";
import { registerSearchProvider, type SearchItem } from "@/features/search/providers";
import { revealLocation } from "@/features/lsp/navigation";
import { CLASS_KINDS, searchKind, uniqueSymbols } from "./model";

/** One backend request serves both tabs of the same query (the "All" tab asks both). */
let last: { query: string; at: number; result: Promise<WorkspaceSymbol[]> } | null = null;

export function querySymbols(query: string): Promise<WorkspaceSymbol[]> {
  const now = Date.now();
  if (last && last.query === query && now - last.at < 1500) return last.result;
  const result = langserversApi
    .workspaceSymbols(query)
    .then(uniqueSymbols)
    .catch(() => [] as WorkspaceSymbol[]);
  last = { query, at: now, result };
  return result;
}

export function symbolItem(s: WorkspaceSymbol, query: string): SearchItem | null {
  const m = fuzzyMatch(query, s.name, { strict: false });
  const file = s.path.split("/").pop() ?? s.path;
  const line = s.range ? s.range.start.line + 1 : undefined;
  const dir = s.path.split("/").slice(0, -1).join("/");
  return {
    id: `${s.server}:${s.path}:${line ?? 0}:${s.name}`,
    label: s.name,
    matches: m?.positions,
    detail: s.containerName ? `${s.containerName} · ${dir || "/"}` : dir,
    hint: line ? `${file}:${line}` : file,
    kind: searchKind(s.kind),
    // Servers already ranked their hits; keep their order within equal fuzzy scores.
    score: (m?.score ?? 0) - 1,
    run: async () => {
      if (s.range) await revealLocation(s.path, s.range.start, s.range.end);
      else await revealLocation(s.path, { line: 0, character: 0 });
    },
  };
}

function toItems(symbols: WorkspaceSymbol[], query: string, limit: number): SearchItem[] {
  return symbols
    .map((s) => symbolItem(s, query))
    .filter((i): i is SearchItem => i !== null)
    .slice(0, limit);
}

export function registerSymbolProviders(): void {
  registerSearchProvider({
    id: "lsp.symbols",
    tab: "symbols",
    title: "Symbols",
    order: 50,
    async search(query, { limit }) {
      if (query.trim().length < 2) return [];
      return toItems(await querySymbols(query.trim()), query.trim(), limit);
    },
  });
  registerSearchProvider({
    id: "lsp.classes",
    tab: "classes",
    title: "Classes",
    order: 50,
    async search(query, { limit }) {
      if (query.trim().length < 2) return [];
      const symbols = (await querySymbols(query.trim())).filter((s) => CLASS_KINDS.has(s.kind));
      return toItems(symbols, query.trim(), limit);
    },
  });
}
