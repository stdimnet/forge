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
