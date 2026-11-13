/**
 * Browser-mode Related Symbol (`code_related`): the Rust implementation
 * (`forge_codeintel::related`) as WebAssembly over the demo project's files.
 */
import { registerMock } from "@/ipc";
import type { RelatedItem } from "@/ipc/codeintel";
import { callWasm, forgeWasm } from "@/wasm";
import { mockFiles } from "./workspaceMock";

registerMock("code_related", async (args): Promise<RelatedItem[]> => {
  const { path, content } = args as { path: string; content?: string };
  const w = await forgeWasm();
  return callWasm<RelatedItem[]>(w.codeintel_related, { path, text: content ?? mockFiles[path] ?? "", files: mockFiles });
});
