/**
 * Browser-mode Run Anything, launch plans and External Tools: the Rust `forge-runtools`
 * crate as WebAssembly, fed with what the desktop backend reads from the project (npm and
 * Composer scripts, PHP files of the demo). The history lives in memory.
 */
import { registerMock } from "@/ipc";
import type { HistoryEntry, SuggestRequest } from "@/ipc/runTools";
import { callWasm, forgeWasm } from "@/wasm";
import { mockFiles } from "./workspaceMock";
import { matchFiles } from "./fuzzyMock";
import { mockNpmPackages } from "./webMock";
import { projectRoot } from "./runMock";
import { mockSettingValue } from "./settingsMock";

const projectName = () => projectRoot().split("/").pop() ?? "";

let history: HistoryEntry[] = [{ command: "php artisan migrate --seed" }, { command: "UserTest", configuration: true }];

registerMock("run_anything_suggest", async ({ request }) => {
  const wasm = await forgeWasm();
  const req = { ...(request as SuggestRequest) } as SuggestRequest & Record<string, unknown>;
  const query = req.query.trimStart();
  if (!query.startsWith("?")) {
    req.npm = (await mockNpmPackages())
      .filter((p) => p.scripts.length)
      .map((p) => ({ dir: p.dir, packageManager: p.packageManager.manager, scripts: p.scripts.map((s) => s.name) }));
    const composer = mockFiles["composer.json"];
    req.composerScripts = composer ? (JSON.parse(wasm.composer_scripts(composer)) as string[]) : [];
    if (query.startsWith("php")) {
      const hits = await matchFiles(query.slice(3).trim(), Object.keys(mockFiles), 90);
      req.phpFiles = hits
        .map((h) => h.path)
        .filter((p) => p.endsWith(".php") && !p.endsWith(".blade.php"))
        .slice(0, 30);
    }
  }
  return callWasm(wasm.run_anything_suggest, req);
});

registerMock("run_anything_resolve", async ({ command, cwd }) =>
  callWasm((await forgeWasm()).run_anything_resolve, { command, cwd: cwd ?? "" }),
);

registerMock("run_anything_history", () => structuredClone(history));
registerMock("run_anything_history_add", async ({ entry }) => {
  history = await callWasm<HistoryEntry[]>((await forgeWasm()).run_anything_push_history, { history, entry });
  return structuredClone(history);
});

registerMock("run_launch_plan", async ({ configurations, target }) => {
  const setting = String(mockSettingValue("web.packageManager") ?? "auto");
  return callWasm((await forgeWasm()).run_launch_plan, {
    configurations,
    target,
    packageManager: setting === "auto" ? "npm" : setting,
  });
});

const context = (ctx: unknown) => ({ context: ctx ?? {}, root: projectRoot(), name: projectName() });

registerMock("external_tool_macros", async ({ context: ctx }) =>
  callWasm((await forgeWasm()).external_tool_macros, {
    ...context({ phpExecutable: "/usr/bin/php", osName: "linux", userHome: "~", ...(ctx as object) }),
  }),
);
registerMock("external_tool_command", async ({ tool, context: ctx }) =>
  callWasm((await forgeWasm()).external_tool_command, {
    tool,
    ...context({ phpExecutable: "/usr/bin/php", osName: "linux", userHome: "~", ...(ctx as object) }),
  }),
);
registerMock("external_tool_validate", async ({ tool, others }) =>
  callWasm((await forgeWasm()).external_tool_validate, { tool, others: others ?? [] }),
);
