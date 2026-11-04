/**
 * Web code quality tools, like PHPStorm's Settings | Languages & Frameworks | JavaScript |
 * Code Quality Tools and Prettier: ESLint, Prettier, Stylelint and Biome.
 *
 * - The settings page (`WebQualityToolsPage`).
 * - On the fly: ESLint, Stylelint and Biome check the edited file after a pause in typing
 *   (through stdin, so unsaved text is checked), when it opens and after it is saved.
 * - Problems go to the editor and the Problems tool window (`problems.ts`); Alt+Enter
 *   offers "Fix ESLint problems", "Fix Stylelint problems" and "Fix Biome problems".
 * - Code | Reformat with Prettier (Ctrl+Alt+Shift+P), Fix ESLint / Stylelint / Biome
 *   Problems, Inspect Code with ESLint, Stylelint and Biome; Reformat Code uses Prettier or
 *   Biome when "Run on Reformat Code action" is on.
 * - Fix on save runs in Rust through Actions on Save (`forge-save-actions`).
 *
 * Rust finds the packages and configuration files, runs the tools and parses their reports
 * (`commands/web_quality_tools.rs`, `webforge-quality-tools`).
 */
import { createEffect, createRoot, on } from "solid-js";
import { ViewPlugin, type ViewUpdate } from "@codemirror/view";
import type { ContextAction } from "@/ipc/codeintel";
import type { WebTool } from "@/ipc/webQualityTools";
import { registerEditorExtension } from "@/editor/extensions";
import { registerContextActionHandler, registerContextActionProvider } from "@/features/codeintel/intentions";
import { registerFormatProvider } from "@/features/lsp/format";
import { action, registerMainMenuEntries, separator } from "@/features/mainMenu/model";
import { registerSettingsPage } from "@/features/settingsPages";
import { registerKeymapActions } from "@/keymap";
import { registerCommand } from "@/state/commands";
import { activeTab, editor, onFileSaved } from "@/state/editor";
import { getSetting } from "@/state/settings";
import { safeMode } from "@/features/trust/store";
import { viewFor } from "@/editor/views";
import { isPluginActive } from "@/state/project";
import { clearAll, problemsOf } from "./problems";
import { WebQualityToolsPage } from "./WebQualityToolsPage";
import { active, ALL_TOOLS, checkFile, fixWith, formatWithWebTools, inspectWithWebTools, isWebFile, onTheFly, PAGE } from "./run";
import { t } from "@/i18n";

export { inspectWithWebTools } from "./run";

registerSettingsPage({
  id: PAGE,
  title: t("settings.page.code-quality-tools"),
  group: "Languages & Frameworks",
  order: 22,
  keywords: ["eslint", "prettier", "stylelint", "biome", "lint", "linter", "code quality", "formatter", "fix on save", "run on save", "javascript", "css"],
  component: WebQualityToolsPage,
});

// ---- On the fly --------------------------------------------------------------------------

/** Pause after the last keystroke before the tools check the text. */
const DELAY_MS = 1000;

registerEditorExtension(({ path }) => {
  if (!isWebFile(path)) return null;
  return ViewPlugin.define((view) => {
    let timer: ReturnType<typeof setTimeout> | undefined;
    const opened = setTimeout(() => void checkFile(path, view.state.doc.toString()), 300);
    return {
      update(u: ViewUpdate) {
        if (!u.docChanged || !onTheFly()) return;
        clearTimeout(timer);
        timer = setTimeout(() => void checkFile(path, view.state.doc.toString()), DELAY_MS);
      },
      destroy() {
        clearTimeout(timer);
        clearTimeout(opened);
      },
    };
  });
});

/** The settings that change what the tools report. */
const configSignature = () =>
  ALL_TOOLS.map((t) => ["mode", "path", "config", "options", "workingDirectories", "files"].map((o) => String(getSetting(`web.${t}.${o}`, ""))).join(",")).join("|") +
  `|${getSetting("web.biome.priority", true)}|${safeMode()}`;

/** After the tools' settings changed (or Safe Mode): check the open files again. */
function recheckOpenFiles(): void {
  clearAll();
  if (!active()) return;
  for (const tab of editor.tabs) {
    const view = viewFor(tab.path);
    if (isWebFile(tab.path) && view) void checkFile(tab.path, view.state.doc.toString());
  }
}

if (typeof document !== "undefined") {
  createRoot(() => createEffect(on(configSignature, recheckOpenFiles, { defer: true })));
  onFileSaved((path) => {
    // After the Actions on Save tools (eslint --fix, Prettier) rewrote the file.
    if (isWebFile(path)) setTimeout(() => void checkFile(path, viewFor(path)?.state.doc.toString() ?? null), 200);
  });
}

// ---- Reformat Code -----------------------------------------------------------------------

registerFormatProvider(formatWithWebTools);

// ---- Alt+Enter ---------------------------------------------------------------------------

const FIX_TITLES: Partial<Record<WebTool, string>> = {
  eslint: "Fix ESLint problems",
  stylelint: "Fix Stylelint problems",
  biome: "Fix Biome problems",
};

registerContextActionProvider(async (path, view) => {
  if (!active() || !isWebFile(path)) return [];
  const line = view.state.doc.lineAt(view.state.selection.main.head).number - 1;
  const tools = new Set<WebTool>();
  for (const p of problemsOf(path)) {
    if (!p.fixable || line < p.line || line > (p.endLine ?? p.line)) continue;
    tools.add(p.tool);
  }
  return [...tools]
    .filter((t) => FIX_TITLES[t])
    .map(
      (tool): ContextAction => ({
        title: FIX_TITLES[tool]!,
        kind: "quickFix",
        source: "web-quality-tools",
        isPreferred: true,
        needsResolve: false,
        action: "WebQualityTools.Fix",
        data: { tool },
      }),
    );
});

registerContextActionHandler("WebQualityTools.Fix", (data, ctx) => fixWith((data as { tool: WebTool }).tool, ctx.path));

// ---- Code menu ---------------------------------------------------------------------------

const COMMANDS: { id: string; tool: WebTool; title: string; menu: string }[] = [
  { id: "ReformatWithPrettierAction", tool: "prettier", title: "Reformat with Prettier", menu: "Reformat with _Prettier" },
  { id: "WebQualityTools.FixEslint", tool: "eslint", title: "Fix ESLint Problems", menu: "Fix _ESLint Problems" },
  { id: "WebQualityTools.FixStylelint", tool: "stylelint", title: "Fix Stylelint Problems", menu: "Fix St_ylelint Problems" },
  { id: "WebQualityTools.FixBiome", tool: "biome", title: "Fix Biome Problems", menu: "Fix _Biome Problems" },
];

registerKeymapActions([
  {
    id: "ReformatWithPrettierAction",
    title: "Reformat with Prettier",
    group: "Plugins/JavaScript",
    shortcuts: { $default: ["Ctrl+Alt+Shift+P"], macOS: ["Cmd+Opt+Shift+P"] },
  },
  { id: "WebQualityTools.FixEslint", title: "Fix ESLint Problems", group: "Plugins/JavaScript" },
  { id: "WebQualityTools.FixStylelint", title: "Fix Stylelint Problems", group: "Plugins/JavaScript" },
  { id: "WebQualityTools.FixBiome", title: "Fix Biome Problems", group: "Plugins/JavaScript" },
  { id: "WebQualityTools.InspectCode", title: "Inspect Code with ESLint, Stylelint and Biome", group: "Plugins/JavaScript" },
]);

for (const c of COMMANDS) {
  registerCommand({
    id: c.id,
    title: c.title,
    category: "Code",
    run: () => {
      const tab = activeTab();
      if (tab && isWebFile(tab.path)) void fixWith(c.tool, tab.path);
    },
  });
}

registerCommand({
  id: "WebQualityTools.InspectCode",
  title: "Inspect Code with ESLint, Stylelint and Biome",
  category: "Code",
  run: () => void inspectWithWebTools(null),
});

registerMainMenuEntries(
  "_Code",
  [separator, ...COMMANDS.map((c) => action(c.id, c.menu)), action("WebQualityTools.InspectCode", "Inspect Code with ESLint, Stylelint and Biome")],
  { when: () => isPluginActive("forge.web") },
);

