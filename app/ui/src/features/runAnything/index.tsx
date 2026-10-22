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
import { laravelApi } from "@/ipc/laravel";
import { symfonyApi } from "@/ipc/symfony";
import { cliToolsApi } from "@/ipc/cliTools";
import { showBalloon } from "@/keymap";
import { RunAnything } from "./RunAnything";
import { registerRunAnythingProvider } from "./providers";
import { raOpen, toggleRunAnything } from "./state";
import "./runAnything.css";

export { registerRunAnythingProvider, type RunAnythingProvider } from "./providers";
export { toggleRunAnything } from "./state";

registerCommand({
  id: "RunAnything",
  title: "Run Anything",
  category: "Tools",
  run: () => {
    if (!workspace()) return void showBalloon("Run Anything", "Open a project first.");
    toggleRunAnything();
  },
});

registerRunAnythingProvider({
  id: "artisan",
  title: "Artisan commands",
  when: () => usesFramework("laravel"),
  commands: async () =>
    (await laravelApi.artisanCommands()).commands.map((c) => ({ command: `php artisan ${c.name}`, description: c.description })),
});

registerRunAnythingProvider({
  id: "symfony",
  title: "Symfony console commands",
  when: () => usesFramework("symfony"),
  commands: async () =>
    (await symfonyApi.consoleCommands())
      .filter((c) => !c.hidden)
      .map((c) => ({ command: `php bin/console ${c.name}`, description: c.description })),
});

// Composer commands as Command Line Tool Support knows them (its cached `composer list`).
registerRunAnythingProvider({
  id: "composer-commands",
  title: "Composer commands",
  when: () => isPluginActive("forge.command-line-tools"),
  commands: async () => {
    const tool = (await cliToolsApi.list()).find((t) => t.kind === "composer" && t.enabled);
    if (!tool) return [];
    const list = await cliToolsApi.commands(tool.alias);
    return list.commands
      .filter((c) => !c.hidden && !c.group)
      .map((c) => ({ command: `composer ${c.name}`, description: c.description }));
  },
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
