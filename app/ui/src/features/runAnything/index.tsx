/**
 * Run Anything (Tools | Run Anything…, double Ctrl): run or debug configurations, npm and
 * Composer scripts, PHP files, Artisan / Symfony console commands and any shell command from
 * one popup. Other features add commands with `registerRunAnythingProvider`.
 */
import { Show } from "solid-js";
import { render } from "solid-js/web";
import { registerCommand } from "@/state/commands";
import { workspace } from "@/state/workspace";
import { isPluginActive, usesFramework } from "@/state/project";
import { symfonyApi } from "@/ipc/symfony";
import { cliToolsApi } from "@/ipc/cliTools";
import { showBalloon } from "@/keymap";
import { RunAnything } from "./RunAnything";
import { registerRunAnythingProvider } from "./providers";
import { raOpen, toggleRunAnything } from "./state";
import "./runAnything.css";

export { registerRunAnythingProvider, type RunAnythingProvider } from "./providers";
import { t as tr } from "@/i18n";
export { toggleRunAnything } from "./state";

registerCommand({
  id: "RunAnything",
  title: "Run Anything",
  category: "Tools",
  run: () => {
    if (!workspace()) return void showBalloon(tr("tools.runAnything.runAnything"), tr("tools.runAnything.openAProjectFirst"));
    toggleRunAnything();
  },
});

/** How a Command Line Tool Support tool's commands are typed in Run Anything. */
const CLI_PREFIX: Partial<Record<string, (path: string) => string>> = {
  composer: () => "composer",
  artisan: (path) => `php ${path || "artisan"}`,
  symfony: (path) => `php ${path || "bin/console"}`,
};

const CLI_TITLES: Record<string, string> = {
  composer: "Composer commands",
  artisan: "Artisan commands",
  symfony: "Symfony console commands",
};

// Composer, Artisan and Symfony Console commands from Command Line Tool Support (its cached
// command lists); each kind is its own group.
for (const kind of Object.keys(CLI_PREFIX)) {
  registerRunAnythingProvider({
    id: kind,
    title: CLI_TITLES[kind],
    when: () => isPluginActive("forge.command-line-tools"),
    commands: async () => {
      const tool = (await cliToolsApi.list()).find((t) => t.kind === kind && t.enabled);
      if (!tool) return [];
      const prefix = CLI_PREFIX[kind]!(tool.path);
      return (await cliToolsApi.commands(tool.alias)).commands
        .filter((c) => !c.hidden && !c.group)
        .map((c) => ({ command: `${prefix} ${c.name}`, description: c.description }));
    },
  });
}

// Without Command Line Tool Support, Symfony projects still offer the console commands of
// the Symfony index.
registerRunAnythingProvider({
  id: "symfony-index",
  title: tr("tools.runAnything.symfonyConsoleCommands"),
  when: () => usesFramework("symfony") && !isPluginActive("forge.command-line-tools"),
  commands: async () =>
    (await symfonyApi.consoleCommands())
      .filter((c) => !c.hidden)
      .map((c) => ({ command: `php bin/console ${c.name}`, description: c.description })),
});

if (typeof document !== "undefined") {
  const host = document.createElement("div");
  host.id = "run-anything-overlay";
  document.body.appendChild(host);
  render(
    () => (
      <Show when={raOpen()}>
        <RunAnything />
      </Show>
    ),
    host,
  );
}
