/**
 * Browser-mode Minecraft plugins (JavaForge): the `minecraft_*` commands on the demo plugin
 * (`?project=minecraft`, demoMinecraftProject.ts) and the files of the New Project wizard's
 * Minecraft Plugin generator. Generation, the descriptor check, Ctrl+B on `main:` and the
 * gutter markers go through the Rust `javaforge-minecraft` crate (WebAssembly), like in the
 * desktop app. "Refresh versions" answers with canned versions: no network.
 */
import { registerMock } from "@/ipc";
import type { CreateArgs, Generator, GeneratorChoice, OptionValue } from "@/ipc/project";
import { defaultOptions, isOptionVisible } from "@/features/new-project/wizard";
import { callWasmSync, forgeWasm } from "@/wasm";
import { isMinecraftDemo, minecraftDemoInfo } from "./demoMinecraftProject";
import { mockFiles } from "./workspaceMock";
import generators from "./projectGenerators.json";

if (isMinecraftDemo()) {
  registerMock("project_detect", () => structuredClone(minecraftDemoInfo));
}

const gens = generators as unknown as Generator[];

/** Options as the backend uses them: defaults filled in, hidden options at their default. */
export function effectiveOptions(gen: Generator, options: Record<string, OptionValue> | undefined): Record<string, OptionValue> {
  const values = { ...defaultOptions(gen), ...(options ?? {}) };
  for (const o of gen.options) if (!isOptionVisible(o, values, gen)) values[o.id] = o.default;
  return values;
}

/** The files of the Minecraft Plugin generator (or the error shown under the form). */
export async function minecraftFiles(args: CreateArgs): Promise<{ files: string[]; error?: string }> {
  const gen = gens.find((g) => g.id === "minecraft-plugin")!;
  const wasm = await forgeWasm();
  try {
    const out = callWasmSync<{ path: string }[]>(wasm.minecraft_generate, {
      name: args.name.trim(),
      options: effectiveOptions(gen, args.options),
    });
    return { files: out.map((f) => f.path) };
  } catch (e) {
    return { files: [], error: String(e).replace(/^Error: /, "") };
  }
}

/** Versions "Refresh versions" pretends Paper published. */
const REFRESHED = ["1.21.11", "1.21.10", "1.21.9", "1.21.8", "1.21.7", "1.21.6", "1.21.5", "1.21.4", "1.21.3", "1.21.1", "1.21"];

registerMock("minecraft_versions_refresh", async (): Promise<GeneratorChoice[]> => {
  await new Promise((r) => setTimeout(r, 400));
  return callWasmSync<GeneratorChoice[]>((await forgeWasm()).minecraft_version_choices, { versions: REFRESHED });
});

const request = (args: Record<string, unknown>) => ({ ...args, files: mockFiles });

registerMock("minecraft_inspect", async (args) => callWasmSync((await forgeWasm()).minecraft_inspect, request(args)));
registerMock("minecraft_goto", async (args) => callWasmSync((await forgeWasm()).minecraft_goto, request(args)));
registerMock("minecraft_gutter", async (args) => callWasmSync((await forgeWasm()).minecraft_gutter, request(args)));
