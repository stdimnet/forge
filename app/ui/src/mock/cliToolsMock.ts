/**
 * Browser-mode Command Line Tool Support: the demo project's tools (detected from its
 * files plus `commandLineTools.tools`), command lists recorded in Symfony Console's
 * `list --format=json` format, and canned runs streamed as `run://` events. Detection,
 * list parsing, completion and the command lines are computed by the Rust
 * `phpforge-cli-tools` crate (WebAssembly) exactly like in the desktop app. Nothing runs and
 * nothing is written.
 */
import { registerMock } from "@/ipc";
import { toolName, type CliTool, type CommandDef, type ToolCommands } from "@/ipc/cliTools";
import { callWasm, forgeWasm } from "@/wasm";
import { mockArtisanCommands, simulatedArtisanOutput } from "./laravelMock";
import { composerSteps, startMockRun, throughInterpreter, type Step } from "./runMock";
import { isSymfonyDemo } from "./demoSymfonyProject";
import { isWordPressDemo } from "./demoWordPressProject";
import { isDrupalDemo } from "./demoDrupalProject";
import { isLuaDemo } from "./demoLuaProject";
import { isPythonDemo } from "./demoPythonProject";
import { isGoDemo } from "./demoGoProject";
import { isJavaDemo } from "./demoJavaProject";
import { isKotlinDemo } from "./demoKotlinProject";
import { webDemo } from "./demoWebProjects";
import { mockSettingValue } from "./settingsMock";

/** Project files the detection looks at. */
function demoFiles(): string[] {
  if (isLuaDemo() || isPythonDemo() || isGoDemo() || isJavaDemo() || isKotlinDemo() || webDemo()) return [];
  if (isSymfonyDemo()) return ["composer.json", "bin/console"];
  if (isWordPressDemo()) return ["composer.json", "vendor/bin/wp"];
  if (isDrupalDemo()) return ["composer.json", "vendor/bin/drush"];
  return ["composer.json", "composer.lock", "artisan"];
}

// ---- Recorded command lists ----------------------------------------------------------

type Opt = [name: string, shortcut: string, value: "" | "required" | "optional", description: string, defaultValue?: string];
type Arg = [name: string, required: boolean, description: string, array?: boolean];

const GLOBALS: Opt[] = [
  ["help", "-h", "", "Display help for the given command. When no command is given display help for the list command"],
  ["silent", "", "", "Do not output any message"],
  ["quiet", "-q", "", "Only errors are displayed. All other output is suppressed"],
  ["verbose", "-v|-vv|-vvv", "", "Increase the verbosity of messages: 1 for normal output, 2 for more verbose output and 3 for debug"],
  ["version", "-V", "", "Display this application version"],
  ["ansi", "", "", "Force (or disable --no-ansi) ANSI output"],
  ["no-ansi", "", "", 'Negate the "--ansi" option'],
  ["no-interaction", "-n", "", "Do not ask any interactive question"],
];

function command(name: string, description: string, args: Arg[], opts: Opt[], globals: Opt[], aliases: string[] = []) {
  const arguments_: Record<string, unknown> = {
    command: { name: "command", is_required: true, is_array: false, description: "The command to execute", default: null },
  };
  for (const [n, required, desc, array] of args)
    arguments_[n] = { name: n, is_required: required, is_array: !!array, description: desc, default: array ? [] : null };
  const options: Record<string, unknown> = {};
  for (const [n, shortcut, value, desc, def] of [...opts, ...globals])
    options[n] = {
      name: `--${n}`,
      shortcut,
      accept_value: value !== "",
      is_value_required: value === "required",
      is_multiple: false,
      description: desc,
      default: value ? (def ?? null) : false,
    };
  const usage = `${name}${opts.length ? " [options]" : ""}${args.length ? ` [--] ${args.map(([n, r]) => (r ? `<${n}>` : `[<${n}>]`)).join(" ")}` : ""}`;
  return { name, hidden: false, usage: [usage], description, help: description, aliases, definition: { arguments: arguments_, options } };
}

const listText = (app: string, version: string, commands: unknown[]) => JSON.stringify({ application: { name: app, version }, commands });

const ARTISAN_ARGS: Record<string, string> = {
  name: "The name of the class",
  class: "The class name of the root seeder",
  model: "The model to show",
  config: "The configuration file or key to show",
  connection: "The name of the queue connection to work",
  store: "The name of the store you would like to clear",
};

const ARTISAN_OPTIONS: Record<string, Opt> = {
  all: ["all", "-a", "", "Generate a migration, seeder, factory, policy, resource controller, and form request classes for the model"],
  controller: ["controller", "-c", "", "Create a new controller for the model"],
  factory: ["factory", "-f", "", "Create a new factory for the model"],
  migration: ["migration", "-m", "", "Create a new migration file for the model"],
  seed: ["seed", "-s", "", "Indicates if the seed task should be re-run"],
  force: ["force", "", "", "Force the operation to run when in production"],
  pretend: ["pretend", "", "", "Dump the SQL queries that would be run"],
  step: ["step", "", "", "Force the migrations to be run so they can be rolled back individually"],
  json: ["json", "", "", "Output as JSON"],
  method: ["method", "", "optional", "Filter the routes by method"],
  name: ["name", "", "optional", "Filter the routes by name"],
  path: ["path", "", "optional", "Only show routes matching the given path pattern"],
  host: ["host", "", "optional", "The host address to serve the application on", "127.0.0.1"],
  port: ["port", "", "optional", "The port to serve the application on"],
  queue: ["queue", "", "optional", "The names of the queues to work"],
  tries: ["tries", "", "optional", "Number of times to attempt a job before logging it failed", "1"],
  table: ["table", "", "optional", "The table to migrate"],
  create: ["create", "", "optional", "The table to be created"],
  only: ["only", "", "optional", "The section to display"],
  filter: ["filter", "", "required", "Filter which tests to run"],
};

function artisanList(): string {
  const env: Opt = ["env", "", "optional", "The environment the command should run under"];
  const commands = mockArtisanCommands.map((c) =>
    command(
      c.name,
      c.description,
      c.arguments.map((a): Arg => [a, a === "name", ARTISAN_ARGS[a] ?? ""]),
      c.options.map((o): Opt => ARTISAN_OPTIONS[o] ?? [o, "", "", ""]),
      [...GLOBALS, env],
    ),
  );
  commands.push(
    command("help", "Display help for a command", [["command_name", false, "The command name"]], [["format", "", "required", "The output format (txt, xml, json, or md)", "txt"]], GLOBALS),
    command("list", "List commands", [["namespace", false, "The namespace name"]], [["format", "", "required", "The output format (txt, xml, json, or md)", "txt"]], GLOBALS),
  );
  return listText("Laravel Framework", "12.4.1", commands);
}

function composerList(): string {
  const globals: Opt[] = [
    ...GLOBALS,
    ["profile", "", "", "Display timing and memory usage information"],
    ["no-plugins", "", "", "Whether to disable plugins."],
    ["no-scripts", "", "", "Skips the execution of all scripts defined in composer.json file."],
    ["working-dir", "-d", "required", "If specified, use the given directory as working directory."],
    ["no-cache", "", "", "Prevent use of the cache"],
  ];
  const packages: Arg = ["packages", false, "Packages that should be updated, if not provided all packages are.", true];
  return listText("Composer", "2.8.9", [
    command("install", "Installs the project dependencies from the composer.lock file if present, or falls back on the composer.json", [], [
      ["dry-run", "", "", "Outputs the operations but will not execute anything (implicitly enables --verbose)."],
      ["no-dev", "", "", "Disables installation of require-dev packages."],
      ["optimize-autoloader", "-o", "", "Optimize autoloader during autoloader dump"],
      ["prefer-dist", "", "", "Forces installation from package dist (default behavior)."],
    ], globals, ["i"]),
    command("require", "Adds required packages to your composer.json and installs them", [["packages", false, "Optional package name can also include a version constraint, e.g. foo/bar or foo/bar:1.0.0", true]], [
      ["dev", "", "", "Add requirement to require-dev."],
      ["no-update", "", "", "Disables the automatic update of the dependencies (implies --no-install)."],
      ["update-with-dependencies", "-w", "", "Allows inherited dependencies to be updated, except those that are root requirements."],
    ], globals, ["r"]),
    command("update", "Updates your dependencies to the latest version according to composer.json, and updates the composer.lock file", [packages], [
      ["with-all-dependencies", "-W", "", "Update also dependencies of packages in the argument list, including those which are root requirements."],
      ["lock", "", "", "Overwrites the lock file hash to suppress warning about the lock file being out of date without updating package versions."],
      ["dry-run", "", "", "Outputs the operations but will not execute anything (implicitly enables --verbose)."],
    ], globals, ["u", "upgrade"]),
    command("remove", "Removes a package from the require or require-dev", [["packages", false, "Packages that should be removed.", true]], [["dev", "", "", "Removes a package from the require-dev section."]], globals, ["rm", "uninstall"]),
    command("dump-autoload", "Dumps the autoloader", [], [["optimize", "-o", "", "Optimizes PSR0 and PSR4 packages to be loaded with classmaps too, good for production."]], globals, ["dumpautoload"]),
    command("show", "Shows information about packages", [["package", false, "Package to inspect. Or a name including a wildcard (*) to filter lists of packages instead."], ["version", false, "Version or version constraint to inspect"]], [
      ["tree", "-t", "", "List the dependencies as a tree"],
      ["latest", "-l", "", "Show the latest version"],
      ["format", "-f", "required", "Format of the output: text or json", "text"],
    ], globals, ["info"]),
    command("outdated", "Shows a list of installed packages that have updates available, including their latest version", [["package", false, "Package to inspect. Or a name including a wildcard (*) to filter lists of packages instead."]], [["direct", "-D", "", "Shows only packages that are directly required by the root package"]], globals),
    command("validate", "Validates a composer.json and composer.lock", [["file", false, "path to composer.json file"]], [["strict", "", "", "Return a non-zero exit code for warnings as well as errors"]], globals),
    command("audit", "Checks for security vulnerability advisories for installed packages", [], [["format", "-f", "required", "Output format. Must be \"table\", \"plain\", \"json\", or \"summary\".", "table"]], globals),
    command("run-script", "Runs the scripts defined in composer.json", [["script", false, "Script name to run."]], [["list", "-l", "", "List scripts."]], globals, ["run"]),
  ]);
}

function symfonyList(): string {
  const globals: Opt[] = [...GLOBALS, ["env", "-e", "required", "The Environment name.", "dev"], ["no-debug", "", "", "Switch off debug mode."]];
  return listText("Symfony", "7.3.1", [
    command("about", "Display information about the current project", [], [], globals),
    command("cache:clear", "Clear the cache", [], [["no-warmup", "", "", "Do not warm up the cache"]], globals),
    command("debug:router", "Display current routes for an application", [["name", false, "A route name"]], [["show-controllers", "", "", "Show assigned controllers in overview"]], globals),
    command("debug:container", "Display current services for an application", [["name", false, "A service name (foo)"]], [["tag", "", "required", "Show all services with a specific tag"]], globals),
    command("doctrine:migrations:migrate", "Execute a migration to a specified version or the latest available version", [["version", false, "The version FQCN or alias (first, prev, next, latest) to migrate to.", false]], [["dry-run", "", "", "Execute the migration as a dry run."]], globals),
    command("doctrine:schema:update", "Executes (or dumps) the SQL needed to update the database schema to match the current mapping metadata", [], [
      ["dump-sql", "", "", "Dumps the generated SQL statements to the screen (does not execute them)."],
      ["force", "-f", "", "Causes the generated SQL statements to be physically executed against your database."],
    ], globals),
    command("make:controller", "Create a new controller class", [["controller-class", false, "Choose a name for your controller class (e.g. AgreeableGnomeController)"]], [], globals),
    command("make:entity", "Create or update a Doctrine entity class, and optionally an API Platform resource", [["name", false, "Class name of the entity to create or update"]], [["regenerate", "", "", "Instead of adding new fields, simply generate the methods (e.g. getter/setter) for existing fields"]], globals),
    command("help", "Display help for a command", [["command_name", false, "The command name"]], [], globals),
    command("list", "List commands", [["namespace", false, "The namespace name"]], [], globals),
  ]);
}

/** A short `wp cli cmd-dump` (WP-CLI's own format). */
function wpCliDump(): string {
  const cmd = (name: string, description: string, synopsis = "", subcommands: unknown[] = []) => ({ name, description, synopsis, longdesc: "", subcommands });
  return JSON.stringify({
    name: "wp",
    description: "Manage WordPress through the command-line.",
    longdesc: "",
    subcommands: [
      cmd("cache", "Adds, removes, fetches, and flushes the WP Object Cache object.", "", [cmd("flush", "Flushes the object cache."), cmd("get", "Gets a value from the object cache.", "<key> [<group>] [--format=<format>]")]),
      cmd("cron", "Tests, runs, and deletes WP-Cron events; manages WP-Cron schedules.", "", [cmd("event", "Schedules, runs, and deletes WP-Cron events.", "", [cmd("list", "Lists scheduled cron events.", "[--fields=<fields>] [--format=<format>]"), cmd("run", "Runs the next scheduled cron event for the given hook.", "[<hook>...] [--due-now] [--all]")])]),
      cmd("option", "Retrieves and sets site options, including plugin and WordPress settings.", "", [cmd("get", "Gets the value for an option.", "<key> [--format=<format>]"), cmd("update", "Updates an option value.", "<key> [<value>] [--autoload=<autoload>] [--format=<format>]")]),
      cmd("plugin", "Manages plugins, including installs, activations, and updates.", "", [cmd("activate", "Activates one or more plugins.", "[<plugin>...] [--all] [--network]"), cmd("deactivate", "Deactivates one or more plugins.", "[<plugin>...] [--uninstall] [--all] [--network]"), cmd("list", "Gets a list of plugins.", "[--status=<status>] [--fields=<fields>] [--format=<format>]")]),
      cmd("post", "Manages posts, content, and meta.", "", [cmd("create", "Creates a new post.", "[--post_type=<post_type>] [--post_title=<post_title>] [--post_status=<post_status>] [--porcelain]"), cmd("list", "Gets a list of posts.", "[--post_type=<post_type>] [--fields=<fields>] [--format=<format>]")]),
      cmd("rewrite", "Lists or flushes the site's rewrite rules, updates the permalink structure.", "", [cmd("flush", "Flushes rewrite rules.", "[--hard]")]),
    ],
  });
}

const LISTS: Partial<Record<CliTool["kind"], () => string>> = {
  artisan: artisanList,
  composer: composerList,
  symfony: symfonyList,
  "wp-cli": wpCliDump,
};

// ---- IPC -----------------------------------------------------------------------------

async function tools(): Promise<CliTool[]> {
  const wasm = await forgeWasm();
  return callWasm<CliTool[]>(wasm.cli_tools_effective, {
    configured: mockSettingValue("commandLineTools.tools") ?? [],
    autoDetect: mockSettingValue("commandLineTools.autoDetect") ?? true,
    files: demoFiles(),
    composer: String(mockSettingValue("php.composer.executable") ?? ""),
  });
}

const loaded = new Map<string, CommandDef[]>();

async function commandsOf(tool: CliTool): Promise<ToolCommands> {
  const list = LISTS[tool.kind];
  if (!list) {
    return tool.kind === "php-script"
      ? { alias: tool.alias, commands: [], source: "none" }
      : { alias: tool.alias, commands: [], source: "none", error: `${tool.path} list --format=json: Could not open input file: ${tool.path}` };
  }
  const wasm = await forgeWasm();
  const commands = loaded.get(tool.alias) ?? (await callWasm<CommandDef[]>(wasm.cli_tools_parse_list, { kind: tool.kind, text: list() }));
  const source = loaded.has(tool.alias) ? "cache" : "tool";
  loaded.set(tool.alias, commands);
  return { alias: tool.alias, commands, source };
}

registerMock("cli_tools_list", () => tools());

registerMock("cli_tools_commands", async ({ alias, reload }) => {
  const tool = (await tools()).find((t) => t.alias === alias);
  if (!tool) throw new Error(`unknown tool alias '${String(alias)}'`);
  if (reload) loaded.delete(tool.alias);
  return commandsOf(tool);
});

registerMock("cli_tools_complete", async ({ input, offset }) => {
  const all = await tools();
  const alias = String(input).trim().split(/\s+/)[0] ?? "";
  const tool = all.find((t) => t.enabled && t.alias === alias);
  const commands: Record<string, CommandDef[]> = {};
  if (tool) commands[alias] = (await commandsOf(tool)).commands;
  const wasm = await forgeWasm();
  return callWasm(wasm.cli_tools_complete, { input, offset, tools: all, commands });
});

const lines = (text: string, stream: "stdout" | "stderr" = "stdout", delay = 40): Step => [delay, text, stream];

function symfonySteps(args: string[]): Step[] {
  const [name] = args;
  if (name === "cache:clear")
    return [
      lines('\n // Clearing the cache for the \x1b[32mdev\x1b[39m environment with debug \x1b[32mtrue\x1b[39m\n\n', "stdout", 300),
      lines(' \x1b[30;42m[OK] Cache for the "dev" environment (debug=true) was successfully cleared.\x1b[39;49m\n\n', "stdout", 500),
      [20, { code: 0 }],
    ];
  return [lines(`\n \x1b[30;42m[OK] ${name ?? "list"} finished.\x1b[39;49m\n\n`, "stdout", 200), [20, { code: 0 }]];
}

function wpCliSteps(args: string[]): Step[] {
  const line = args.join(" ");
  if (line.startsWith("plugin list"))
    return [
      lines("+-------------+----------+-----------+---------+\n| name        | status   | update    | version |\n+-------------+----------+-----------+---------+\n", "stdout", 200),
      lines("| acme-events | active   | none      | 1.2.0   |\n| akismet     | inactive | available | 5.3.3   |\n+-------------+----------+-----------+---------+\n", "stdout", 80),
      [20, { code: 0 }],
    ];
  if (line.startsWith("cache flush") || line.startsWith("rewrite flush"))
    return [lines(`\x1b[32mSuccess:\x1b[0m ${line.startsWith("cache") ? "The cache was flushed." : "Rewrite rules flushed."}\n`, "stdout", 250), [20, { code: 0 }]];
  return [lines(`\x1b[32mSuccess:\x1b[0m wp ${line} finished.\n`, "stdout", 200), [20, { code: 0 }]];
}

registerMock("cli_tools_run", async ({ input, runId }) => {
  const all = await tools();
  const wasm = await forgeWasm();
  const parsed = await callWasm<{
    tool: CliTool;
    args: string[];
    invocation: { program: { type: "php" } | { type: "other"; name: string }; args: string[]; env: Record<string, string> };
  }>(wasm.cli_tools_parse_input, { input, tools: all });
  const { tool, args, invocation } = parsed;
  const php = invocation.program.type === "php";
  const local = { program: php ? "php" : (invocation.program as { name: string }).name, args: invocation.args, cwd: "", env: invocation.env };
  const { cmd, pathMappings } = php
    ? await throughInterpreter({ name: tool.alias, type: "php-script", interpreter: tool.interpreter }, local)
    : { cmd: local, pathMappings: [] };
  let steps: Step[];
  if (tool.kind === "artisan") {
    const out = simulatedArtisanOutput(args.join(" "));
    steps = [...out.lines.map(([text, stderr]): Step => [25, `${text}\n`, stderr ? "stderr" : "stdout"]), [20, { code: out.code }]];
  } else if (tool.kind === "composer") steps = composerSteps(args);
  else if (tool.kind === "symfony") steps = symfonySteps(args);
  else if (tool.kind === "wp-cli") steps = wpCliSteps(args);
  else steps = [lines(`Could not open input file: ${tool.path}\n`, "stderr", 120), [20, { code: 1 }]];
  const started = await startMockRun(String(runId), cmd, steps, { pathMappings });
  return { ...started, alias: tool.alias, tool: toolName(tool), showConsoleIn: tool.showConsoleIn };
});
