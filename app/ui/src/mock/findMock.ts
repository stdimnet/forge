/**
 * Browser-mode Find in Files and symbol search: the in-memory demo project (without
 * `.gitignore`d files) is searched by the same Rust code as the desktop backend
 * (`forge-find` and `forge-search-core`, WebAssembly).
 */
import { registerMock } from "@/ipc";
import type { FileMatches, FindPaged, FindRequest, FindResult } from "@/ipc/find";
import type { SearchStatus, SymbolHit } from "@/ipc/search";
import { callWasm, forgeWasm } from "@/wasm";
import { createIgnoreMatcher } from "./gitignoreMatcher";
import { mockFiles } from "./workspaceMock";

/** `[path, text]` of the demo project without `.gitignore`d files. */
function projectFiles(): [string, string][] {
  const ignores = createIgnoreMatcher((dir) => mockFiles[dir ? `${dir}/.gitignore` : ".gitignore"]);
  return Object.keys(mockFiles)
    .filter((p) => !ignores.isIgnored(p, false))
    .sort()
    .map((p) => [p, mockFiles[p]]);
}

registerMock("find_in_files", async ({ request }) => {
  const w = await forgeWasm();
  return callWasm<FindResult>(w.find_in_files, { request: request as FindRequest, files: projectFiles() });
});

/** The demo language server answers for these, so the index leaves them out like the desktop app does. */
const LSP_SERVED = ["js", "jsx", "ts", "tsx", "mjs", "cjs"];

/** The result `find_in_files_paged` last returned, like the desktop app keeps it. */
let lastResult: { id: number; result: FindResult } | null = null;

registerMock("find_in_files_paged", async ({ request, pageSize }) => {
  const w = await forgeWasm();
  const result = await callWasm<FindResult>(w.find_in_files, { request: request as FindRequest, files: projectFiles() });
  const id = (lastResult?.id ?? 0) + 1;
  lastResult = { id, result };
  return callWasm<FindPaged>(w.find_paged, { result, id, count: pageSize });
});

registerMock("find_page", async ({ id, offset, count }) => {
  if (!lastResult || lastResult.id !== id) throw new Error("The search result was replaced by a newer search");
  const w = await forgeWasm();
  return callWasm<FileMatches[]>(w.find_page, { result: lastResult.result, offset, count });
});

registerMock("search_find_symbols", async ({ query, typesOnly, limit, skipExtensions, exact }) => {
  const w = await forgeWasm();
  return callWasm<SymbolHit[]>(w.search_find_symbols, {
    files: projectFiles(),
    query,
    typesOnly,
    exact: exact === true,
    limit,
    skipExtensions: [...((skipExtensions as string[] | undefined) ?? []), ...LSP_SERVED],
  });
});

registerMock("search_status", (): SearchStatus => ({
  enabled: true,
  indexing: false,
  files: projectFiles().length,
  contentFiles: projectFiles().length,
  binaryFiles: 0,
  oversizedFiles: 0,
  lastRefreshMs: Date.now(),
}));
