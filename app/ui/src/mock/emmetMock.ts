/**
 * Browser-mode Emmet and colors: the same Rust crates as the desktop app
 * (`forge-wasm::emmet`), so the mock computes nothing itself.
 */
import { registerMock } from "@/ipc";
import { callWasmSync, forgeWasm, type ForgeWasm } from "@/wasm";

const bridge = (command: string, fn: (w: ForgeWasm) => (json: string) => string) =>
  registerMock(command, async (args) => callWasmSync(fn(await forgeWasm()), command === "colors_parse" ? { text: args.text } : args.request));

bridge("emmet_expand", (w) => w.emmet_expand);
bridge("emmet_wrap", (w) => w.emmet_wrap);
bridge("emmet_edit_point", (w) => w.emmet_edit_point);
bridge("emmet_update_tag", (w) => w.emmet_update_tag);
bridge("emmet_remove_tag", (w) => w.emmet_remove_tag);
bridge("colors_find", (w) => w.colors_find);
bridge("colors_format", (w) => w.colors_format);
bridge("colors_parse", (w) => w.colors_parse);
