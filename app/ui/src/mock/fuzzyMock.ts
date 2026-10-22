/**
 * Browser-mode fuzzy filtering and Search Everywhere ranking: the same Rust code as the
 * desktop app (`forge-fuzzy`), run as WebAssembly.
 */
import { registerMock } from "@/ipc";
import type { FileMatch } from "@/ipc/search";
import { callWasm, forgeWasm } from "@/wasm";

const wasm = () => forgeWasm();

registerMock("fuzzy_filter", async ({ request }) => callWasm((await wasm()).fuzzy_filter, request));
registerMock("search_rank", async ({ request }) => callWasm((await wasm()).search_rank, request));

/** "Go to File" over a list of paths (the demo project instead of a file walk). */
export const matchFiles = async (query: string, paths: string[], limit: number, exact = false): Promise<FileMatch[]> =>
  callWasm<FileMatch[]>((await wasm()).search_match_files, { query, paths, limit, exact });
