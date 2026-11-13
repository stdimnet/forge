/**
 * Browser-mode keymap backend: the `keymap_*` commands run the Rust keymap engine
 * (`forge-keymap`) as WebAssembly. The settings live in the settings mock, so every call
 * passes the current keymap settings and stores the settings writes it returns.
 */
import { registerMock } from "@/ipc";
import type { KeymapSnapshot } from "@/ipc/keymap";
import type { SettingValue } from "@/ipc/settings";
import { callWasm, forgeWasm, type ForgeWasm } from "@/wasm";
import { hasUserSetting, mockSettingValue, setSetting } from "./settingsMock";

const SETTING_KEYS = [
  "keymap.active",
  "keymap.overrides.default",
  "keymap.overrides.macOS",
  "keymap.overrides.bundled",
  "keymap.custom",
];

interface SettingWrite {
  key: string;
  value: SettingValue | null;
}

const hostPlatform = (): "mac" | "other" =>
  typeof navigator !== "undefined" && /Mac|iPhone|iPad/.test(navigator.platform || navigator.userAgent)
    ? "mac"
    : "other";

const context = () => ({
  settings: Object.fromEntries(SETTING_KEYS.map((k) => [k, mockSettingValue(k)])),
  hostPlatform: hostPlatform(),
});

/** Call a keymap bridge function with the current settings plus `args`. */
async function call<T>(fn: (w: ForgeWasm) => (json: string) => string, args: Record<string, unknown> = {}): Promise<T> {
  const wasm = await forgeWasm();
  return callWasm<T>(fn(wasm), { ...context(), ...args });
}

const store = (writes: SettingWrite[]) => writes.forEach((w) => setSetting(w.key, w.value, "user"));

registerMock("keymap_actions", () => call<KeymapSnapshot>((w) => w.keymap_actions));
registerMock("keymap_register_actions", ({ actions, commands }) =>
  call<KeymapSnapshot>((w) => w.keymap_register_actions, { actions, commands }),
);
registerMock("keymap_set_override", async ({ change }) => {
  store(await call<SettingWrite[]>((w) => w.keymap_set_override, { change }));
  return call<KeymapSnapshot>((w) => w.keymap_actions);
});
registerMock("keymap_conflicts", ({ shortcut, actionId }) =>
  call((w) => w.keymap_conflicts, { shortcut, actionId: actionId ?? null }),
);
registerMock("keymap_find_by_shortcut", ({ shortcut }) => call((w) => w.keymap_find_by_shortcut, { shortcut }));
registerMock("keymap_main_menu", ({ request }) => call((w) => w.keymap_main_menu, { request }));
registerMock("keymap_resolve", ({ event }) => call((w) => w.keymap_resolve, { event }));
registerMock("keymap_import_legacy", async ({ active, overrides }) => {
  const wasm = await forgeWasm();
  const writes = (await callWasm<SettingWrite[]>(wasm.keymap_legacy_changes, { active, overrides })).filter(
    (w) => !hasUserSetting(w.key),
  );
  store(writes);
  return writes.map((w) => w.key);
});
