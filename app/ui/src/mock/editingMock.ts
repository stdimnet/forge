/**
 * Browser-mode editor actions: postfix completion, Complete Current Statement and Cyclic
 * Expand Word run through the same Rust code as the desktop app (`forge-editing` as
 * WebAssembly).
 */
import { registerMock } from "@/ipc";
import { callWasm, forgeWasm } from "@/wasm";

registerMock("editor_postfix_templates", async () => callWasm((await forgeWasm()).editing_postfix_templates, {}));
registerMock("editor_postfix_complete", async ({ request }) =>
  callWasm((await forgeWasm()).editing_postfix_complete, request),
);
registerMock("editor_complete_statement", async ({ request }) =>
  callWasm((await forgeWasm()).editing_complete_statement, request),
);
registerMock("editor_expand_word", async ({ request }) => callWasm((await forgeWasm()).editing_expand_word, request));
