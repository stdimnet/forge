/**
 * Open the Command Line Tools Console with a tool's alias typed in (Tools | WP-CLI…,
 * Drush…, Shopware Console…), so its commands complete as usual. Explains, with a link to
 * the settings page, when the project has no such tool.
 */
import { balloonsFor } from "@/keymap";
import { openSettings } from "@/features/settings/dialogState";
import type { ToolKind } from "@/ipc/cliTools";
import { openRunCommand, SETTINGS_PAGE } from "./CommandPopup";
import { loadTools, tools } from "./store";

const showBalloon = balloonsFor("Command Line Tools");

export async function openToolConsole(kind: ToolKind, label: string, missing: string, path?: string): Promise<void> {
  const list = tools().length ? tools() : await loadTools();
  const tool = list.find((t) => t.kind === kind && t.enabled && (!path || t.path === path)) ?? list.find((t) => t.kind === kind && t.enabled);
  if (tool) {
    openRunCommand(`${tool.alias} `);
    return;
  }
  showBalloon(`${label} is not configured`, missing, 8000, [{ label: "Configure…", run: () => openSettings(SETTINGS_PAGE) }]);
}
