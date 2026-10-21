/**
 * Browser-mode search backend: `fs_find_files` over the demo project. Classes, Symbols and
 * Text come from the same Rust code as the desktop app (`search_find_symbols` and
 * `find_in_files` in `findMock.ts`, WebAssembly).
 */
import { registerMock } from "@/ipc";
import { demoFiles } from "./demoProject";
import { matchFiles } from "./fuzzyMock";
import { isLuaDemo, luaDemoFiles } from "./demoLuaProject";

/** Files of the open demo project (the Lua demo has its own). */
const projectFiles = isLuaDemo() ? luaDemoFiles : demoFiles;

registerMock("fs_find_files", ({ query, limit }) =>
  matchFiles(String(query ?? ""), Object.keys(projectFiles), Number(limit ?? 100)),
);
