/**
 * Browser-mode scopes: the demo project's files and the test folders of its
 * `composer.json`, read by the same Rust code as the desktop app (`forge-scopes`,
 * WebAssembly).
 */
import { registerMock } from "@/ipc";
import { callWasm, forgeWasm } from "@/wasm";
import { projectFiles } from "./findMock";
import { mockFiles } from "./workspaceMock";

registerMock("scopes_files", ({ limit }) => {
  const files = projectFiles().map(([p]) => p);
  return typeof limit === "number" ? files.slice(0, limit) : files;
});

registerMock("scopes_test_roots", async () => {
  const w = await forgeWasm();
  return callWasm<string[]>(w.scopes_composer_test_roots, { composerJson: mockFiles["composer.json"] ?? "{}" });
});
