import { describe, expect, it } from "vitest";
import { invoke, listen } from "@/ipc";
import type { CliCompletion, CliRunStarted, CliTool, ToolCommands } from "@/ipc/cliTools";
import { runEvent, type RunEvent } from "@/ipc/run";
import { validateEntry } from "@/features/cliTools/ToolsEditor";
import "./runMock";
import "./cliToolsMock";

describe("Command Line Tools mock (Rust through WebAssembly)", () => {
  it("detects Composer and Artisan in the demo project", async () => {
    const tools = await invoke<CliTool[]>("cli_tools_list");
    expect(tools.map((t) => [t.alias, t.kind, t.path, t.detected])).toEqual([
      ["c", "composer", "composer", true],
      ["a", "artisan", "artisan", true],
    ]);
  });

  it("parses the recorded command lists", async () => {
    const artisan = await invoke<ToolCommands>("cli_tools_commands", { alias: "a", reload: true });
    expect(artisan.source).toBe("tool");
    const model = artisan.commands.find((c) => c.name === "make:model")!;
    expect(model.arguments?.map((a) => a.name)).toEqual(["name"]);
    expect(model.options?.find((o) => o.name === "migration")?.shortcuts).toEqual(["m"]);
    const again = await invoke<ToolCommands>("cli_tools_commands", { alias: "a", reload: false });
    expect(again.source).toBe("cache");
  });

  it("completes commands, options and arguments", async () => {
    const cmds = await invoke<CliCompletion>("cli_tools_complete", { input: "a mig", offset: 5 });
    expect(cmds.items.slice(0, 2).map((i) => i.label)).toEqual(["migrate", "migrate:fresh"]);
    const opts = await invoke<CliCompletion>("cli_tools_complete", { input: "c require --d", offset: 13 });
    expect(opts.items.map((i) => i.label)).toContain("--dev");
    expect(opts.help?.command.name).toBe("require");
    const arg = await invoke<CliCompletion>("cli_tools_complete", { input: "a make:model ", offset: 13 });
    expect(arg.hint).toBe("<name>: The name of the class");
    expect(arg.help?.active).toEqual({ kind: "argument", name: "name" });
  });

  it("runs a command and streams its console", async () => {
    const events: RunEvent[] = [];
    const off = await listen<RunEvent>(runEvent("cli-t1"), (e) => events.push(e));
    const started = await invoke<CliRunStarted>("cli_tools_run", { input: "a make:model Order", runId: "cli-t1" });
    expect(started.commandLine).toBe("php artisan make:model Order --ansi");
    expect(started.tool).toBe("Laravel Artisan");
    await new Promise((r) => setTimeout(r, 400));
    off();
    const text = events.flatMap((e) => (e.type === "console" ? e.lines.flatMap((l) => l.segments.map((s) => s.text)) : [])).join("");
    expect(text).toContain("app/Models/Order.php");
    expect(events.at(-1)).toMatchObject({ type: "exited", code: 0 });
    await expect(invoke("cli_tools_run", { input: "x foo", runId: "cli-t2" })).rejects.toThrow(/Unknown tool alias 'x'/);
  });

  it("validates tool entries", () => {
    expect(validateEntry({ kind: "artisan", alias: "" }, [])).toBe("Specify an alias.");
    expect(validateEntry({ kind: "artisan", alias: "a b" }, [])).toBe("The alias must not contain spaces.");
    expect(validateEntry({ kind: "artisan", alias: "a" }, [{ kind: "composer", alias: "a" }])).toBe("The alias 'a' is already used.");
    expect(validateEntry({ kind: "php-script", alias: "t", path: "" }, [])).toBe("Specify the path to the script.");
    expect(validateEntry({ kind: "composer", alias: "c", path: "composer" }, [])).toBeNull();
  });
});
