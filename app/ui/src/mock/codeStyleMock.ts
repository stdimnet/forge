/**
 * Browser-mode Code Style: schemes, previews, import/export and the Reformat Code pass run
 * through the same Rust code as the desktop app (`forge-codestyle` as WebAssembly); schemes
 * are stored in the mock settings layers. Files are never written: export returns the text.
 */
import { registerMock } from "@/ipc";
import type { CodeStyleSchemes } from "@/ipc/codeStyle";
import type { SettingsScope, SettingValue } from "@/ipc/settings";
import { callWasm, forgeWasm } from "@/wasm";
import { mockSettingLayers, setSetting } from "./settingsMock";

interface SettingWrite {
  key: string;
  value: SettingValue | null;
  scope: SettingsScope;
}

registerMock("code_style_model", async () => callWasm((await forgeWasm()).codestyle_model, null));

registerMock("code_style_schemes", async () => callWasm((await forgeWasm()).codestyle_schemes, mockSettingLayers()));

registerMock("code_style_save", async ({ schemes }) => {
  const wasm = await forgeWasm();
  const { writes } = await callWasm<{ writes: SettingWrite[] }>(wasm.codestyle_plan_save, {
    ...mockSettingLayers(),
    schemes: schemes as CodeStyleSchemes,
  });
  for (const w of writes) setSetting(w.key, w.value, w.scope);
  return callWasm(wasm.codestyle_schemes, mockSettingLayers());
});

registerMock("code_style_preview", async ({ language, values }) =>
  callWasm((await forgeWasm()).codestyle_preview, { language, values }),
);

registerMock("code_style_export", async ({ name, values, format }) =>
  callWasm((await forgeWasm()).codestyle_export, { name, values, format }),
);

registerMock("code_style_import", async ({ text, path }) => {
  if (typeof text !== "string") throw new Error(`cannot read ${String(path)} in browser mode`);
  return callWasm((await forgeWasm()).codestyle_import, { text });
});

registerMock("code_style_post_format", async ({ request }) =>
  callWasm((await forgeWasm()).codestyle_post_format, { ...mockSettingLayers(), request }),
);
