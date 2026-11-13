/**
 * Browser-mode JSON Schema: the same Rust crate as the desktop app (`forge-wasm::json_schema`)
 * with the bundled schemas and the demo project's files. Remote schemas are not downloaded.
 */
import { registerMock } from "@/ipc";
import type { SchemaMapping } from "@/ipc/jsonSchema";
import { callWasmSync, forgeWasm } from "@/wasm";
import { mockSettingValue } from "./settingsMock";
import { mockFiles } from "./workspaceMock";

type Fn = (json: string) => string;

/** Run a request, adding the project schema files the answer asks for. */
async function run<T>(fn: (w: Awaited<ReturnType<typeof forgeWasm>>) => Fn, args: Record<string, unknown>): Promise<T | null> {
  const w = await forgeWasm();
  const mappings = (mockSettingValue("jsonSchema.mappings") as SchemaMapping[] | null) ?? [];
  const files: Record<string, string> = {};
  for (let round = 0; round < 8; round++) {
    const out = callWasmSync<{ answer?: T | null; needs?: string[] }>(fn(w), { ...args, mappings, files });
    if (!out.needs) return out.answer ?? null;
    for (const path of out.needs) files[path] = mockFiles[path] ?? "";
  }
  return null;
}

registerMock("json_schema_validate", async (args) => (await run((w) => w.json_schema_validate, args)) ?? { schema: null, problems: [] });
registerMock("json_schema_complete", (args) => run((w) => w.json_schema_complete, args));
registerMock("json_schema_hover", (args) => run((w) => w.json_schema_hover, args));
registerMock("json_schema_goto", async (args) => (await run((w) => w.json_schema_goto, args)) ?? []);
registerMock("json_schema_read", async ({ location }) => {
  const out = JSON.parse((await forgeWasm()).json_schema_read(String(location))) as { text?: string; detail?: string; error?: string };
  if (out.error) throw new Error(out.error);
  return out;
});
registerMock("json_schema_builtins", async () => JSON.parse((await forgeWasm()).json_schema_builtins()));
