/**
 * Browser-mode Actions on Save: the plan and "Changed lines" run the Rust code as
 * WebAssembly; the PHP actions only sort and prune `use` lines (the tree-sitter based Rust
 * code is not in the browser build). The external tools are reported as found or missing;
 * the PHP fixers the demo project "has" (Pint, phpcbf, Rector) change the in-memory demo
 * file like the quality tools mock does, so "Pint on save" can be seen. Nothing real is
 * ever written.
 */
import { invoke, registerMock } from "@/ipc";
import type { PhpActions, SavePlan, SaveTool, ToolLocation, ToolsReport } from "@/ipc/saveActions";
import { callWasm, forgeWasm } from "@/wasm";
import { mockSettingValues } from "./settingsMock";
import { mockFixedText } from "./qualityToolsMock";
import type { QualityTool } from "@/ipc/qualityTools";

/** Tools the demo projects "have" (Laravel ships Pint; the demo package.json has Prettier). */
const DEMO_TOOLS: Record<SaveTool, string | null> = {
  eslint: null,
  prettier: "node_modules/.bin/prettier",
  phpCsFixer: null,
  pint: "vendor/bin/pint",
  phpcbf: "vendor/bin/phpcbf",
  rector: "vendor/bin/rector",
};
const LABELS: Record<SaveTool, string> = {
  eslint: "ESLint",
  prettier: "Prettier",
  phpCsFixer: "PHP CS Fixer",
  pint: "Laravel Pint",
  phpcbf: "phpcbf",
  rector: "Rector",
};
const CANDIDATES: Record<SaveTool, string> = {
  eslint: "node_modules/.bin/eslint",
  prettier: "node_modules/.bin/prettier",
  phpCsFixer: "vendor/bin/php-cs-fixer",
  pint: "vendor/bin/pint",
  phpcbf: "vendor/bin/phpcbf",
  rector: "vendor/bin/rector",
};

/** The quality tool behind a PHP fixer of Actions on Save. */
const PHP_FIXERS: Partial<Record<SaveTool, QualityTool>> = { phpCsFixer: "phpCsFixer", pint: "pint", phpcbf: "phpcs", rector: "rector" };

registerMock("save_actions_plan", async ({ path, trigger }) => {
  const wasm = await forgeWasm();
  return callWasm<SavePlan>(wasm.save_actions_plan, { path, trigger, settings: mockSettingValues() });
});

registerMock("save_actions_changed_lines", async ({ path, original, formatted }) => {
  const base = await invoke<string | null>("git_show", { rev: "HEAD", path }).catch(() => null);
  const wasm = await forgeWasm();
  return callWasm<string>(wasm.save_actions_changed_lines, { base, original, formatted });
});

/** Sort the `use` lines of a block and drop the ones whose short name never appears again. */
export function mockOptimizeImports(text: string, sort: boolean): string {
  const lines = text.split("\n");
  const isUse = (l: string) => /^use [\w\\]+( as \w+)?;$/.test(l);
  const out: string[] = [];
  for (let i = 0; i < lines.length; ) {
    if (!isUse(lines[i])) {
      out.push(lines[i++]);
      continue;
    }
    const block: string[] = [];
    while (i < lines.length && isUse(lines[i])) block.push(lines[i++]);
    const rest = lines.slice(i).join("\n");
    const kept = block.filter((l) => {
      const name = l.replace(/;$/, "").split(/\\| as /).pop()!;
      return new RegExp(`\\b${name}\\b`).test(rest);
    });
    if (sort) kept.sort((a, b) => a.toLowerCase().localeCompare(b.toLowerCase()));
    out.push(...kept);
  }
  return out.join("\n");
}

registerMock("save_actions_php", ({ text, actions }) => {
  const a = actions as PhpActions;
  let out = String(text);
  const applied: string[] = [];
  if (a.codeCleanup) {
    const next = mockOptimizeImports(out, false);
    if (next !== out) applied.push("Code cleanup");
    out = next;
  }
  if (a.optimizeImports) {
    const next = mockOptimizeImports(out, true);
    if (next !== out) applied.push("Optimize imports");
    out = next;
  }
  return { text: out, applied };
});

registerMock("save_actions_run_tools", async ({ path, tools }): Promise<ToolsReport> => {
  const file = String(path);
  const initial = await invoke<string>("fs_read_file", { path: file }).catch(() => null);
  let text = initial;
  const outcomes = (tools as SaveTool[]).map((tool) => {
    if (!DEMO_TOOLS[tool]) {
      return {
        tool,
        label: LABELS[tool],
        status: "missing" as const,
        message: `${LABELS[tool]} is not installed in the project (${CANDIDATES[tool]}); skipped.`,
      };
    }
    const q = PHP_FIXERS[tool];
    if (q && text !== null) text = mockFixedText(q, text);
    return { tool, label: LABELS[tool], status: "ok" as const, message: "" };
  });
  const changed = text !== null && text !== initial;
  if (changed) await invoke("fs_write_file", { path: file, contents: text });
  return { changed, outcomes };
});

registerMock("save_actions_info", async () => ({
  fileTypes: await callWasm((await forgeWasm()).save_actions_file_types, {}),
  tools: (Object.keys(DEMO_TOOLS) as SaveTool[]).map(
    (tool): ToolLocation => ({ tool, label: LABELS[tool], path: DEMO_TOOLS[tool] }),
  ),
}));
