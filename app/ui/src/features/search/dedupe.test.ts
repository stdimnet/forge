/**
 * Search Everywhere with the PHP index and a language server (php-lsp) both answering the
 * Classes tab: each class is listed once, and the server's library hits (its bundled PHP
 * stubs) only show with "Include non-project items".
 */
import { describe, expect, it } from "vitest";
import { registerMock } from "@/ipc";
import type { WorkspaceSymbol } from "@/ipc/langservers";
import type { SymbolHit } from "@/features/php/api";
import { symbolItem as phpItem } from "@/features/php/search";
import { registerSymbolProviders } from "@/features/langservers/symbols";
import { registerSearchProvider, runSearch } from "./providers";
import "@/mock/fuzzyMock";

const controller = "app/Http/Controllers/OrderController.php";

const phpHit: SymbolHit = {
  name: "OrderController",
  fqn: "App\\Http\\Controllers\\OrderController",
  kind: "class",
  container: "App\\Http\\Controllers",
  path: controller,
  offset: 120,
  line: 6,
  column: 6,
  score: 80,
  positions: [0, 1, 2, 3, 4, 5, 6, 7],
};

const lspSymbol = (name: string, path: string, line: number): WorkspaceSymbol => ({
  name,
  kind: "class",
  containerName: "App\\Http\\Controllers",
  path,
  range: { start: { line, character: 6 }, end: { line, character: 6 + name.length } },
  server: "php-lsp",
});

const stub = lspSymbol("DateMalformedIntervalStringException", "/root/.cache/forge/php-lsp/php-lsp-stubs-0.1.0/date/date_c.php", 1700);

/** php-lsp as the backend filters it: stubs only for non-project searches. */
const requests: boolean[] = [];
const named = (query: string) => (name: string) => name.toLowerCase().includes(query.toLowerCase());
registerMock("lsp_workspace_symbols", ({ query, includeNonProject }) => {
  requests.push(Boolean(includeNonProject));
  const project = [lspSymbol("OrderController", controller, 6)];
  return (includeNonProject ? [...project, stub] : project).filter((s) => named(String(query))(s.name));
});

registerSearchProvider({
  id: "php.classes",
  tab: "classes",
  title: "Classes",
  order: 0,
  search: (q) => (named(q)(phpHit.name) ? [phpItem(phpHit, "shop")] : []),
});
registerSymbolProviders();

const labels = async (tab: "all" | "classes", query: string, includeNonProject: boolean) => {
  const groups = await runSearch(tab, query, { includeNonProject, perGroup: 10 });
  return groups.filter((g) => g.provider.tab === "classes").flatMap((g) => g.items.map((i) => `${i.label} — ${i.hint}`));
};

describe("Search Everywhere with the PHP index and php-lsp", () => {
  it("lists a class reported by both once, with the PHP index's details", async () => {
    expect(await labels("all", "OrderCon", false)).toEqual(["OrderController — shop"]);
    expect(await labels("classes", "OrderCon", false)).toEqual(["OrderController — shop"]);
  });

  it("asks the language servers for library symbols only with non-project items", async () => {
    requests.length = 0;
    expect(await labels("classes", "Date", false)).toEqual([]);
    expect(await labels("classes", "Date", true)).toContain(`DateMalformedIntervalStringException — date_c.php:1701`);
    expect(requests).toEqual([false, true]);
  });
});
