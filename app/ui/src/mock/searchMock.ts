/**
 * Browser-mode search backend: `fs_find_files` over the demo project. Classes, Symbols and
 * Text come from the same Rust code as the desktop app (`search_find_symbols` and
 * `find_in_files` in `findMock.ts`, WebAssembly).
 */
import { registerMock } from "@/ipc";
import { mockFiles } from "./workspaceMock";
import { matchFiles } from "./fuzzyMock";

/** Files of the open demo project (including files created in the UI). */
const projectFiles = mockFiles;

registerMock("fs_find_files", ({ query, limit, exact }) =>
  matchFiles(String(query ?? ""), Object.keys(projectFiles), Number(limit ?? 100), exact === true),
);
