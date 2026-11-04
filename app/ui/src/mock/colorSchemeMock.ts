/**
 * Browser-mode color schemes: the `color_scheme_*` commands run `forge-colorscheme` as
 * WebAssembly. The settings live in the settings mock, so every call passes the current
 * values and stores the settings writes that come back.
 */
import { registerMock } from "@/ipc";
import type { SchemesSnapshot } from "@/ipc/colorScheme";
import type { SettingValue } from "@/ipc/settings";
import { callWasm, forgeWasm, type ForgeWasm } from "@/wasm";
import { mockSettingValue, setSetting } from "./settingsMock";

const SETTING_KEYS = ["editor.colorScheme.active", "editor.colorScheme.schemes"];

interface SettingWrite {
  key: string;
  value: SettingValue | null;
}

const settings = () => Object.fromEntries(SETTING_KEYS.map((k) => [k, mockSettingValue(k)]));

async function call<T>(fn: (w: ForgeWasm) => (json: string) => string, args: Record<string, unknown> = {}): Promise<T> {
  const wasm = await forgeWasm();
  return callWasm<T>(fn(wasm), { settings: settings(), ...args });
}

const store = (writes: SettingWrite[]) => writes.forEach((w) => setSetting(w.key, w.value, "user"));

/** The kind of UI theme the scheme that follows the theme depends on. */
const themeOf = (args: Record<string, unknown>) => ({ dark: !!args.dark, highContrast: !!args.highContrast });

const state = (args: Record<string, unknown>) => call<SchemesSnapshot>((w) => w.colorscheme_state, themeOf(args));

registerMock("color_scheme_state", (args) => state(args));
registerMock("color_scheme_change", async (args) => {
  store(await call<SettingWrite[]>((w) => w.colorscheme_apply, { change: args.change, ...themeOf(args) }));
  return state(args);
});
registerMock("color_scheme_import", async (args) => {
  if (typeof args.text !== "string") throw new Error("nothing to import");
  store(await call<SettingWrite[]>((w) => w.colorscheme_import, { text: args.text }));
  return state(args);
});
registerMock("color_scheme_export", ({ name }) => call<string>((w) => w.colorscheme_export, { name }));
