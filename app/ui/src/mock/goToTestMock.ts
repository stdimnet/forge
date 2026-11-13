/**
 * Browser-mode Go to Test and Create New Test (`code_test_targets`, `code_new_test_plan`,
 * `code_create_test`): the Rust implementation (`forge_codeintel::test_nav`) as WebAssembly
 * over the demo project's files. A created test only goes into the in-memory demo project,
 * never into a real one.
 */
import { registerMock } from "@/ipc";
import type { NewTestPlan, TestSource, TestSourceRequest, TestTargets } from "@/ipc/codeintel";
import { callWasm, forgeWasm } from "@/wasm";
import { mockSettingValue } from "./settingsMock";
import { mockFiles } from "./workspaceMock";

const navInput = (args: unknown) => {
  const { path, content } = args as { path: string; content?: string };
  return {
    path,
    text: content ?? mockFiles[path] ?? "",
    composerPath: String(mockSettingValue("php.composer.json") ?? ""),
    files: mockFiles,
  };
};

registerMock("code_test_targets", async (args) => callWasm<TestTargets>((await forgeWasm()).codeintel_test_targets, navInput(args)));

registerMock("code_new_test_plan", async (args) =>
  callWasm<NewTestPlan | null>((await forgeWasm()).codeintel_new_test_plan, navInput(args)),
);

registerMock("code_create_test", async ({ request }) => {
  const file = await callWasm<TestSource>((await forgeWasm()).codeintel_test_source, request as TestSourceRequest);
  const taken = file.path in mockFiles || Object.keys(mockFiles).some((p) => p.startsWith(`${file.path}/`));
  if (taken) throw new Error(`${file.path} already exists`);
  mockFiles[file.path] = file.text;
  return file;
});
