/**
 * Browser-mode TODO scanning and bookmarks: the same Rust code as the desktop app
 * (`forge-find::todo`, `forge-bookmarks`) runs as WebAssembly over the in-memory demo
 * project. Bookmarks live in memory only (the desktop app stores them in Forge's state dir).
 */
import { registerMock } from "@/ipc";
import type { BookmarkOp, Bookmarks, TodoRequest, TodoResult } from "@/ipc/workbench";
import { callWasm, forgeWasm } from "@/wasm";
import { createIgnoreMatcher } from "./gitignoreMatcher";
import { mockFiles } from "./workspaceMock";

registerMock("todo_scan", async ({ request }) => {
  const w = await forgeWasm();
  const ignores = createIgnoreMatcher((dir) => mockFiles[dir ? `${dir}/.gitignore` : ".gitignore"]);
  const files = Object.keys(mockFiles)
    .filter((p) => !ignores.isIgnored(p, false))
    .sort()
    .map((p) => [p, mockFiles[p]]);
  return callWasm<TodoResult>(w.todo_scan, { request: request as TodoRequest, files });
});

let state: Bookmarks | null = null;

/** Demo bookmarks so the tool window is not empty in screenshots. */
const DEMO: BookmarkOp[] = [
  { op: "setMnemonic", path: "app/Http/Controllers/OrderController.php", line: 18, mnemonic: "1" },
  { op: "toggle", path: "app/Models/Order.php", line: 10 },
  { op: "addList", name: "Checkout review" },
];

registerMock("bookmarks_get", async () => {
  if (!state) {
    const w = await forgeWasm();
    const ops = DEMO.filter((o) => !("path" in o) || o.path in mockFiles);
    state = await callWasm<Bookmarks>(w.bookmarks_apply, { state: null, ops });
  }
  return state;
});

registerMock("bookmarks_apply", async ({ ops }) => {
  const w = await forgeWasm();
  state = await callWasm<Bookmarks>(w.bookmarks_apply, { state, ops: ops as BookmarkOp[] });
  return state;
});
