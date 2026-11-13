/**
 * Browser-mode file types: the same Rust detection as the desktop app (`forge-filetypes`),
 * run as WebAssembly over the built-in plugin manifests. Once the module is loaded
 * (`mock/index.ts` waits for it before the UI starts) answers are synchronous, so editors
 * never wait for a file type here.
 */
import { registerMock } from "@/ipc";
import type { DelimiterCatalog, DetectRequest, FileTypeInfo, FileTypeOverride, FileTypeRow, OverrideChoices } from "@/ipc/filetypes";
import { setSyncFileTypeResolver } from "@/editor/fileTypes";
import { callWasmSync, forgeWasm, type ForgeWasm } from "@/wasm";
import builtinPlugins from "./builtinPlugins.json";

const loaded: Promise<ForgeWasm> = forgeWasm().then((wasm) => {
  callWasmSync<number>(wasm.filetypes_load, builtinPlugins);
  setSyncFileTypeResolver((request) => callWasmSync<FileTypeInfo[]>(wasm.filetypes_detect, request));
  return wasm;
});

registerMock("filetypes_detect", async ({ request }) =>
  callWasmSync<FileTypeInfo[]>((await loaded).filetypes_detect, request as DetectRequest),
);

registerMock("filetypes_list", async ({ request }) =>
  callWasmSync<FileTypeRow[]>((await loaded).filetypes_list, request as DetectRequest),
);

registerMock("filetypes_override_choices", async ({ request }) =>
  callWasmSync<OverrideChoices>((await loaded).filetypes_override_choices, request as DetectRequest),
);

registerMock("filetypes_edit_overrides", async ({ request }) =>
  callWasmSync<FileTypeOverride[]>((await loaded).filetypes_edit_overrides, request),
);

registerMock("filetypes_template_delimiters", async () =>
  JSON.parse((await loaded).filetypes_template_delimiters()) as DelimiterCatalog,
);

/** Resolves when file types answer synchronously. */
export const fileTypesReady = loaded.then(() => undefined);
