/**
 * Browser-mode clipboard history: the same Rust model as the desktop app (`forge-editing`
 * as WebAssembly), kept in the page's memory.
 */
import { registerMock } from "@/ipc";
import { callWasm, forgeWasm } from "@/wasm";

registerMock("clipboard_history_add", async (args) => callWasm((await forgeWasm()).clipboard_history_add, args));
registerMock("clipboard_history_list", async (args) => callWasm((await forgeWasm()).clipboard_history_list, args));
registerMock("clipboard_history_remove", async (args) => callWasm((await forgeWasm()).clipboard_history_remove, args));
registerMock("clipboard_history_promote", async (args) => callWasm((await forgeWasm()).clipboard_history_promote, args));

// The system clipboard: the browser's own (it may ask for permission or refuse).
registerMock("clipboard_read_text", async () => {
  if (!navigator.clipboard?.readText) throw new Error("The browser offers no clipboard access");
  return navigator.clipboard.readText();
});
registerMock("clipboard_write_text", async ({ text }) => {
  if (!navigator.clipboard?.writeText) throw new Error("The browser offers no clipboard access");
  await navigator.clipboard.writeText(String(text ?? ""));
});
