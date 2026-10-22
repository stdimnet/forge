/**
 * Browser-mode file templates: the catalog, PSR-4 mapping and rendering run through the same
 * Rust code as the desktop app (`forge-file-templates` as WebAssembly); a new file only goes
 * into the in-memory demo project, never into a real one.
 */
import { registerMock } from "@/ipc";
import type { NewFileRequest } from "@/ipc/templates";
import { callWasm, forgeWasm } from "@/wasm";
import { mockSettingValue } from "./settingsMock";
import { mockFiles, mockProjectName } from "./workspaceMock";

const custom = () => mockSettingValue("fileTemplates.custom") ?? [];

const composer = () => {
  const path = String(mockSettingValue("php.composer.json") || "composer.json");
  // Mark Directory as | Sources Root changes (Settings | Directories).
  return { composerJson: mockFiles[path] ?? null, composerPath: path, sourceRoots: mockSettingValue("php.sourceRoots") ?? [] };
};

registerMock("templates_list", async ({ custom: draft }) =>
  callWasm((await forgeWasm()).templates_list, { custom: draft ?? custom() }),
);
registerMock("templates_validate", async ({ text }) => callWasm((await forgeWasm()).templates_validate, { text }));
registerMock("templates_php_namespace", async ({ dir }) =>
  callWasm((await forgeWasm()).templates_php_namespace, { ...composer(), dir }),
);
registerMock("templates_php_directory", async ({ namespace }) =>
  callWasm((await forgeWasm()).templates_php_directory, { ...composer(), namespace }),
);
registerMock("templates_create", async ({ request }) => {
  const req = request as NewFileRequest;
  const context = { projectName: mockProjectName, user: "marlon", ...req.context };
  const file = await callWasm<{ path: string; text: string }>((await forgeWasm()).templates_new_file, {
    request: { ...req, context },
    custom: custom(),
  });
  const taken = file.path in mockFiles || Object.keys(mockFiles).some((p) => p.startsWith(`${file.path}/`));
  if (taken) throw new Error(`${file.path} already exists`);
  mockFiles[file.path] = file.text;
  return file;
});
