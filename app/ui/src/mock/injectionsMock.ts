/**
 * Browser-mode language injections: the same Rust functions as the desktop commands
 * (`forge-injections`), as WebAssembly.
 */
import { registerMock } from "@/ipc";
import type { ContextAction } from "@/ipc/codeintel";
import type { LiteralKind } from "@/ipc/injections";
import { callWasm, forgeWasm } from "@/wasm";
import { mockSettingValue } from "./settingsMock";

const list = (key: string) => (mockSettingValue(key) as string[] | null) ?? [];

/** The `editor.injections.*` settings, like `commands::injections::settings_of`. */
export const injectionSettings = () => ({
  disabled: list("editor.injections.disabled"),
  uninjected: list("editor.injections.uninjected"),
});

registerMock("injections_find", async (a) => callWasm((await forgeWasm()).injections_find, a.request));
registerMock("injections_at", async (a) => callWasm((await forgeWasm()).injections_at, a.request));
registerMock("injections_fragment", async (a) => callWasm((await forgeWasm()).injections_fragment, a.request));
registerMock("injections_escape", async (a) =>
  callWasm((await forgeWasm()).injections_escape, { kind: a.kind as LiteralKind, text: a.text as string }),
);
registerMock("injections_inject", async (a) => callWasm((await forgeWasm()).injections_inject, a.request));
registerMock("injections_rules", async () => JSON.parse((await forgeWasm()).injections_rules()));
registerMock("injections_languages", async () => JSON.parse((await forgeWasm()).injections_languages()));

/** The injection intentions of `code_context_actions` (Inject language, Un-inject, Edit Fragment). */
export async function injectionContextActions(path: string, text: string, offset: number): Promise<ContextAction[]> {
  const wasm = await forgeWasm();
  return callWasm<ContextAction[]>(wasm.injections_context_actions, { text, path, offset, ...injectionSettings() });
}
