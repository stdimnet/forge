/**
 * Browser-mode Find in Files and symbol search: the in-memory demo project (without
 * `.gitignore`d files) is searched by the same Rust code as the desktop backend
 * (`forge-find` and `forge-search-core`, WebAssembly).
 */
import { registerMock } from "@/ipc";
import type { FindRequest, FindResult } from "@/ipc/find";
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

registerMock("search_find_symbols", async ({ query, typesOnly, limit, skipExtensions }) => {
  const w = await forgeWasm();
  return callWasm<SymbolHit[]>(w.search_find_symbols, {
    files: projectFiles(),
    query,
    typesOnly,
    limit,
    skipExtensions: [...((skipExtensions as string[] | undefined) ?? []), ...LSP_SERVED],
  });
});

registerMock("search_status", (): SearchStatus => ({ enabled: true, indexing: false, files: projectFiles().length }));
