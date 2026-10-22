/**
 * Browser-mode spelling and grammar: the same Rust crate as the desktop app
 * (`forge-wasm::spell`). Custom dictionaries are files on the user's machine, so browser
 * mode lists them as not loaded.
 */
import { registerMock } from "@/ipc";
import { getSetting } from "@/state/settings";
import type { DictionaryInfo } from "@/ipc/spell";
import { callWasmSync, forgeWasm } from "@/wasm";

registerMock("spell_inspect", async (args) => callWasmSync((await forgeWasm()).spell_inspect, args.request));
registerMock("spell_context_actions", async (args) => callWasmSync((await forgeWasm()).spell_context_actions, args.request));
registerMock("spell_dictionaries", async (): Promise<DictionaryInfo[]> => {
  const builtin = JSON.parse((await forgeWasm()).spell_dictionaries()) as DictionaryInfo[];
  const custom = getSetting<string[]>("spelling.customDictionaries", []).filter((p) => p.trim());
  return [
    ...builtin,
    ...custom.map((path) => ({
      name: path.split(/[\\/]/).pop() ?? path,
      path,
      words: 0,
      builtin: false,
      error: "Custom dictionaries are read by the desktop app",
    })),
  ];
});
