import { describe, expect, it } from "vitest";
import { invoke } from "@/ipc";
import type { RunConfiguration } from "@/ipc/run";
import type { ExternalTool, LaunchPlan, MacroInfo, SuggestResponse, ToolCommand } from "@/ipc/runTools";
import { decodeTool, encodeTool, newTool, toolActionId, toolGroups, toolRef } from "@/features/externalTools/model";
import "./runToolsMock";

const suggest = (query: string) =>
  invoke<SuggestResponse>("run_anything_suggest", {
    request: { query, recent: [], configurations: [{ name: "UserTest", type: "phpunit", typeName: "PHPUnit" }], providers: [], laravel: true },
  });

describe("Run Anything (Rust forge-runtools via WebAssembly)", () => {
  it("offers configurations, the demo's scripts and the typed command", async () => {
    const empty = await suggest("");
    expect(empty.groups.map((g) => g.id)).toEqual(["configurations", "npm", "composer"]);
    expect(empty.groups[2].items.map((i) => i.label)).toEqual(["composer lint", "composer test"]);
    const php = await suggest("php User");
    expect(php.groups.find((g) => g.id === "php")?.items[0].label).toBe("php app/Models/User.php");
    const typed = await suggest("ls -la");
    expect(typed.groups[0].items[0].action).toEqual({ kind: "command", command: "ls -la" });
    expect((await suggest("?")).help).toBe(true);
  });

  it("resolves typed commands to configurations", async () => {
    const cfg = await invoke<RunConfiguration>("run_anything_resolve", { command: "php artisan migrate" });
    expect(cfg).toMatchObject({ type: "artisan", options: { command: "migrate" } });
    const shell = await invoke<RunConfiguration>("run_anything_resolve", { command: "echo hi", cwd: "admin" });
    expect(shell).toMatchObject({ type: "shell", workingDirectory: "admin", options: { scriptText: "echo hi" } });
  });

  it("plans compounds and before-launch tasks", async () => {
    const configurations: RunConfiguration[] = [
      { name: "All", type: "compound", options: { configurations: "serve\nassets" } },
      { name: "serve", type: "artisan", options: { command: "serve" }, beforeLaunch: [{ type: "composer", value: "dump-autoload" }] },
      { name: "assets", type: "npm", options: { script: "dev" } },
    ];
    const plan = await invoke<LaunchPlan>("run_launch_plan", { configurations, target: "All" });
    expect(plan.compound).toBe(true);
    expect(plan.runs.map((r) => [r.configuration, r.tasks.map((t) => t.label)])).toEqual([
      ["serve", ["Run Composer script 'dump-autoload'"]],
      ["assets", []],
    ]);
    expect(plan.runs[0].tasks[0].run).toMatchObject({ type: "composer-command", options: { command: "dump-autoload" } });
  });
});

describe("External Tools", () => {
  const tool: ExternalTool = { ...newTool("Linters"), name: "PHPStan", program: "phpstan", arguments: "analyse $FilePathRelativeToProjectRoot$" };

  it("round-trips the settings object and names actions like PHPStorm", () => {
    expect(decodeTool(encodeTool(tool))).toEqual(tool);
    expect(toolActionId(tool)).toBe("Tool_Linters_PHPStan");
    expect(toolRef(tool)).toBe("Linters/PHPStan");
    expect(toolGroups([tool, { ...tool, name: "Off", enabled: false }]).map((g) => g.tools.length)).toEqual([1]);
  });

  it("expands macros and validates in Rust", async () => {
    const cmd = await invoke<ToolCommand>("external_tool_command", { tool, context: { filePath: "app/Models/User.php", line: 3 } });
    expect(cmd.args).toEqual(["analyse", "app/Models/User.php"]);
    expect(cmd.cwd).toBe("~/Projects/acme-shop");
    const macros = await invoke<MacroInfo[]>("external_tool_macros", { context: { filePath: "app/Models/User.php" } });
    expect(macros.find((m) => m.name === "$FileName$")?.preview).toBe("User.php");
    const problems = await invoke<string[]>("external_tool_validate", { tool: { ...tool, outputFilters: ["$LINE$"] }, others: [tool] });
    expect(problems).toHaveLength(2);
  });
});
