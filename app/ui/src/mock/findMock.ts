/**
 * Browser-mode Find in Files: the in-memory demo project (without `.gitignore`d files) is
 * searched by the same Rust code as the desktop backend (`forge-find`, WebAssembly).
 */
import { registerMock } from "@/ipc";
import type { FindRequest, FindResult } from "@/ipc/find";
import { callWasm, forgeWasm } from "@/wasm";
import { createIgnoreMatcher } from "./gitignoreMatcher";
import { mockFiles } from "./workspaceMock";

registerMock("find_in_files", async ({ request }) => {
  const w = await forgeWasm();
  const ignores = createIgnoreMatcher((dir) => mockFiles[dir ? `${dir}/.gitignore` : ".gitignore"]);
  const files = Object.keys(mockFiles)
    .filter((p) => !ignores.isIgnored(p, false))
    .sort()
    .map((p) => [p, mockFiles[p]]);
  return callWasm<FindResult>(w.find_in_files, { request: request as FindRequest, files });
});
