/**
 * RubyForge in the UI (RubyMine's Ruby support). Everything is computed in Rust
 * (`src/ipc/ruby.ts`, crates `packs/rubyforge/*`); this module plugs it into the
 * contribution points:
 *
 * - Settings | Languages & Frameworks | Ruby (Ruby SDK with version managers, gem
 *   executables, Bundler, Rake, RuboCop, Ruby LSP, debugger), the interpreter in the
 *   status bar,
 * - the Rake tool window, the bundle's gems in External Libraries,
 * - run configuration types "Ruby" and "Rake" (RSpec and Minitest come from the manifest's
 *   test frameworks), Debug through rdbg (`ruby_debug_plan`: the run starts under
 *   `rdbg --open`, then the debugger attaches),
 * - actions: Bundle Install / Update / Outdated, Reload Rake Tasks, Run Rake Task, Run
 *   IRB Console (a Terminal tab), Inspect Code with RuboCop, RuboCop Autocorrect, Install
 *   Ruby LSP,
 * - ▶ on scripts, RuboCop problems and autocorrect, Reformat Code (`editor.ts`).
 */
import { createEffect, createRoot, on } from "solid-js";
import { Gem, Hammer, Terminal } from "lucide-solid";
import { RDBG_PORT_ENV, rubyApi, type BundlerAction } from "@/ipc/ruby";
import type { CommandLine, RunConfiguration } from "@/ipc/run";
import { registerKeymapActions } from "@/keymap";
import { registerSettingsPage } from "@/features/settingsPages";
import { registerStatusItem } from "@/features/statusItems";
import { registerToolWindow } from "@/features/registry";
import { refreshDebugAdapters, registerDebugAdapter } from "@/features/debug/adapters";
import { dapApi, type LaunchRequest } from "@/features/debug/dapApi";
import { openSettings } from "@/features/settings/dialogState";
import { action, registerMainMenuEntries } from "@/features/mainMenu/model";
import { registerDebugLauncher, registerRunConfigurationType, type DebugLaunchContext } from "@/features/run/types";
import { openTerminalWith } from "@/features/terminal";
import { setInputDialog } from "@/features/git/overlays";
import { isDialogWindow } from "@/shell/window/mode";
import { registerCommand } from "@/state/commands";
import { activeTab, onFileSaved } from "@/state/editor";
import { showToolWindow } from "@/state/layout";
import { workspace } from "@/state/workspace";
import { RakeToolWindow, rakeConfiguration } from "./RakeToolWindow";
import { RubySettingsPage } from "./RubySettingsPage";
import { autocorrect, checkWithRubocop, isRubyPath } from "./editor";
import { installGem, installRubyLsp } from "./install";
import "./libraries";
import {
  errorText,
  interpreterLabel,
  PLUGIN,
  rakeTasks,
  refreshBundle,
  refreshRake,
  refreshRubyStatus,
  rubyActive,
  rubyBundle,
  rubyStatus,
  runRubyCommand,
  showRubyBalloon,
} from "./state";
import "./ruby.css";

const opt = (cfg: RunConfiguration, key: string) => cfg.options?.[key] ?? "";
const baseName = (p: string) => p.replace(/\\/g, "/").split("/").pop() ?? p;

// ---- Settings, status bar, tool windows ------------------------------------------------------

registerSettingsPage({
  id: "lang.ruby",
  title: "Ruby",
  group: "Languages & Frameworks",
  order: 28,
  keywords: ["ruby", "sdk", "gems", "rbenv", "rvm", "chruby", "asdf", "mise", "bundler", "gemfile", "rake", "rubocop", "ruby-lsp", "rdbg", "debug", "irb", ".ruby-version"],
  component: RubySettingsPage,
});

function RubyStatusItem() {
  return (
    <button
      class="status-item"
      data-status="ruby-interpreter"
      title={rubyStatus()?.chosen ? `${rubyStatus()!.chosen!.interpreter.path}. Click to configure the Ruby SDK` : "No Ruby interpreter. Click to configure"}
      onClick={() => openSettings("lang.ruby")}
    >
      <Gem size={12} />
      <span>{interpreterLabel(rubyStatus())}</span>
    </button>
  );
}

registerStatusItem({ id: "ruby.interpreter", align: "right", order: 87, component: RubyStatusItem, when: rubyActive });

registerToolWindow({
  id: "rake",
  title: "Rake",
  icon: Hammer,
  anchor: "right",
  order: 42,
  component: RakeToolWindow,
  when: () => rubyActive() && (rakeTasks()?.tasks.length ?? 0) > 0,
});

// ---- Run configurations --------------------------------------------------------------------

const bundleExecField = {
  key: "bundleExec",
  label: "Run with bundle exec",
  kind: "enum" as const,
  default: "true",
  options: [
    { value: "true", label: "When the project has a Gemfile" },
    { value: "false", label: "Never" },
  ],
};

registerRunConfigurationType({
  id: "ruby",
  name: "Ruby",
  description: "Run a Ruby script with the project's interpreter, through bundle exec in Bundler projects.",
  icon: Gem,
  color: "var(--danger)",
  pluginId: PLUGIN,
  interpreter: "custom",
  defaultInterpreter: () => rubyStatus()?.chosen?.interpreter.path ?? "ruby",
  argumentsLabel: "Script arguments",
  debuggable: true,
  fields: [{ key: "script", label: "Ruby script", kind: "path", required: true, placeholder: "bin/report.rb" }, bundleExecField],
  suggestName: (cfg) => (opt(cfg, "script") ? baseName(opt(cfg, "script")) : undefined),
});

registerRunConfigurationType({
  id: "rake",
  name: "Rake",
  description: "Run a Rake task with its arguments.",
  icon: Hammer,
  color: "var(--warning)",
  pluginId: PLUGIN,
  argumentsLabel: "Rake options",
  debuggable: true,
  fields: [
    { key: "task", label: "Task name", required: true, placeholder: "db:seed" },
    { key: "taskArgs", label: "Arguments", placeholder: "2026-10 (rake task[arg1,arg2])" },
    bundleExecField,
  ],
  suggestName: (cfg) => (opt(cfg, "task") ? `rake ${opt(cfg, "task")}` : undefined),
});

registerRunConfigurationType({
  id: "ruby-command",
  name: "Ruby",
  icon: Terminal,
  hidden: true,
  fields: [],
  argumentsLabel: false,
  buildCommand: (cfg) => JSON.parse(cfg.options?.commandLine ?? "{}") as CommandLine,
});

// ---- Debugging with rdbg ---------------------------------------------------------------------

registerDebugAdapter({ type: "rdbg", label: "Ruby (rdbg)", languages: ["rb", "rake", "ru", "gemspec"] });

const RDBG_TYPES = new Set(["ruby", "rake", "rspec", "minitest", "rails"]);
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

/** Attach to rdbg once it listens (Ruby and the bundle load first). */
async function attach(request: LaunchRequest, exited: () => boolean, timeoutMs = 60_000): Promise<boolean> {
  const deadline = Date.now() + timeoutMs;
  let last = "";
  while (Date.now() < deadline && !exited()) {
    try {
      await dapApi.start(request);
      return true;
    } catch (e) {
      last = errorText(e);
      await sleep(250);
    }
  }
  if (!exited()) showRubyBalloon("Cannot attach to rdbg", last || "It did not listen in time.", 8000);
  return false;
}

/** Debug a Ruby, Rake, RSpec or Minitest configuration with rdbg. */
async function debugRuby(cfg: RunConfiguration, ctx: DebugLaunchContext): Promise<void> {
  let plan;
  try {
    plan = await rubyApi.debugPlan();
  } catch (e) {
    showRubyBalloon(`Cannot debug '${cfg.name}'`, errorText(e), 8000);
    return;
  }
  if (!plan.found && !(await installGem("debug", "Debugging Ruby needs rdbg, the debugger of the debug gem."))) return;
  await refreshDebugAdapters();
  const handle = await ctx.run({ env: { [RDBG_PORT_ENV]: String(plan.port) } });
  if (!handle) return;
  let done = false;
  void handle.exited.then(() => (done = true));
  showToolWindow("debug");
  await attach({ mode: "attachServer", adapter: "rdbg", host: "127.0.0.1", port: plan.port, name: cfg.name }, () => done);
}

registerDebugLauncher({
  id: "rdbg",
  canDebug: (cfg) => RDBG_TYPES.has(cfg.type) && rubyActive(),
  launch: debugRuby,
});

// ---- Actions ------------------------------------------------------------------------------------

async function bundler(actionId: BundlerAction, gem?: string): Promise<void> {
  try {
    const code = await runRubyCommand(await rubyApi.bundleCommand(actionId, gem));
    if (code === 0 && actionId !== "outdated" && actionId !== "check") await refreshBundle();
  } catch (e) {
    showRubyBalloon(`bundle ${actionId}`, errorText(e), 6000);
  }
}

export async function runRakeTask(task: string, args?: string): Promise<void> {
  const cfg = rakeConfiguration({ name: task });
  if (args) cfg.options = { ...cfg.options, taskArgs: args };
  const { launch } = await import("@/features/run/runStore");
  const { useTemporary } = await import("@/features/run/configStore");
  await launch(useTemporary(cfg));
}

/** The IRB console in a Terminal tab (`bundle exec irb -I lib`). */
export async function openIrb(): Promise<void> {
  try {
    const cmd = await rubyApi.irbCommand();
    openTerminalWith({ title: "IRB", program: cmd.program, args: cmd.args, env: Object.entries(cmd.env ?? {}) });
  } catch (e) {
    showRubyBalloon("Cannot start the IRB console", errorText(e), 8000);
  }
}

registerKeymapActions([
  { id: "ruby.bundleInstall", title: "Bundle Install", group: "Plugins/Ruby" },
  { id: "ruby.bundleUpdate", title: "Bundle Update", group: "Plugins/Ruby" },
  { id: "ruby.bundleOutdated", title: "Show Outdated Gems (bundle outdated)", group: "Plugins/Ruby" },
  { id: "ruby.reloadRake", title: "Reload Rake Tasks", group: "Plugins/Ruby" },
  { id: "ruby.runRakeTask", title: "Run Rake Task…", group: "Plugins/Ruby" },
  { id: "ruby.irbConsole", title: "Run IRB Console", group: "Plugins/Ruby" },
  { id: "ruby.rubocop", title: "Inspect Code with RuboCop", group: "Plugins/Ruby" },
  { id: "ruby.rubocopAutocorrect", title: "RuboCop: Autocorrect File", group: "Plugins/Ruby" },
  { id: "ruby.installRubyLsp", title: "Install Ruby LSP…", group: "Plugins/Ruby" },
  { id: "ActivateRakeToolWindow", title: "Rake", group: "Tool Windows" },
]);

registerCommand({ id: "ruby.bundleInstall", title: "Bundle Install", category: "Ruby", run: () => void bundler("install") });
registerCommand({ id: "ruby.bundleUpdate", title: "Bundle Update", category: "Ruby", run: () => void bundler("update") });
registerCommand({ id: "ruby.bundleOutdated", title: "Show Outdated Gems (bundle outdated)", category: "Ruby", run: () => void bundler("outdated") });
registerCommand({ id: "ruby.reloadRake", title: "Reload Rake Tasks", category: "Ruby", run: () => void refreshRake(true) });
registerCommand({
  id: "ruby.runRakeTask",
  title: "Run Rake Task…",
  category: "Ruby",
  run: () => {
    setInputDialog({
      title: "Run Rake Task",
      label: `Task (${(rakeTasks()?.tasks ?? []).slice(0, 4).map((t) => t.name).join(", ") || "e.g. test"}):`,
      value: "",
      okLabel: "Run",
      onOk: (value) => {
        const m = value.trim().match(/^([^[\s]+)(?:\[(.*)\])?$/);
        if (m) void runRakeTask(m[1], m[2]);
      },
    });
  },
});
registerCommand({ id: "ruby.irbConsole", title: "Run IRB Console", category: "Ruby", run: () => void openIrb() });
registerCommand({
  id: "ruby.rubocop",
  title: "Inspect Code with RuboCop",
  category: "Ruby",
  run: async () => {
    const tab = activeTab();
    if (!tab || !isRubyPath(tab.path)) return void showRubyBalloon("RuboCop", "Open a Ruby file.", 4000);
    await checkWithRubocop(tab.path, undefined, false);
    showToolWindow("problems");
  },
});
registerCommand({
  id: "ruby.rubocopAutocorrect",
  title: "RuboCop: Autocorrect File",
  category: "Ruby",
  run: () => {
    const tab = activeTab();
    if (tab && isRubyPath(tab.path)) void autocorrect(tab.path, "safe");
  },
});
registerCommand({ id: "ruby.installRubyLsp", title: "Install Ruby LSP…", category: "Ruby", run: () => void installRubyLsp() });
registerCommand({ id: "ActivateRakeToolWindow", title: "Rake", category: "View", run: () => showToolWindow("rake") });

registerMainMenuEntries(
  "Tools",
  [
    action("ruby.irbConsole", "Run _IRB Console"),
    action("ruby.runRakeTask", "Run Ra_ke Task..."),
    action("ruby.bundleInstall", "Bundler: _Install"),
    action("ruby.bundleUpdate", "Bundler: _Update"),
  ],
  { when: rubyActive },
);
registerMainMenuEntries("Code", [action("ruby.rubocop", "Inspect Code with Ru_boCop"), action("ruby.rubocopAutocorrect", "RuboCop: _Autocorrect File")], {
  when: rubyActive,
});

// ---- Startup --------------------------------------------------------------------------------------

if (typeof document !== "undefined" && !isDialogWindow()) {
  createRoot(() => {
    createEffect(
      on(
        () => [!!workspace() && rubyActive(), workspace()?.root] as const,
        ([active]) => {
          if (!active) return;
          void refreshBundle().then((b) => {
            if (b?.missing.length)
              showRubyBalloon("Gems are not installed", `${b.missing.slice(0, 3).join(", ")}${b.missing.length > 3 ? "…" : ""} not in Gemfile.lock.`, 10000, [
                { label: "bundle install", run: () => void bundler("install") },
              ]);
          });
          void refreshRake();
          void refreshRubyStatus().then((s) => {
            if (!s) return;
            if (!s.chosen)
              showRubyBalloon("No Ruby interpreter found", "Install Ruby with rbenv, rvm, chruby, asdf or mise, then configure the Ruby SDK.", 10000, [
                { label: "Configure…", run: () => openSettings("lang.ruby") },
              ]);
            else if (s.request && s.chosen.reason !== "versionFile" && !s.setting)
              showRubyBalloon(`Ruby ${s.request.version} is not installed`, `${s.request.file} asks for it; Forge uses ${s.chosen.interpreter.name}.`, 10000, [
                { label: "Configure…", run: () => openSettings("lang.ruby") },
              ]);
            else if (!s.lsp.path)
              showRubyBalloon("Ruby LSP is not installed", "Code insight for Ruby needs Ruby LSP.", 10000, [{ label: "Install…", run: () => void installRubyLsp() }]);
          });
        },
      ),
    );
  });
  // Gemfile or lock file saved: read the bundle again.
  onFileSaved((path) => {
    if (rubyActive() && /(^|\/)(Gemfile|Gemfile\.lock|gems\.rb|gems\.locked)$/.test(path)) void refreshBundle();
    if (rubyActive() && /(^|\/)(Rakefile|[^/]+\.rake)$/.test(path)) void refreshRake();
  });
}

/** For tests and screenshots: the bundle and Rake signals. */
export { rubyBundle, rakeTasks };
