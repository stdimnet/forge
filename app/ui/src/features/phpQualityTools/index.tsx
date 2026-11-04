/**
 * PHP quality tools, like PHPStorm's Settings | PHP | Quality Tools: PHPStan, Psalm, PHP CS
 * Fixer, PHP_CodeSniffer, PHP Mess Detector, Laravel Pint and Rector.
 *
 * - The settings page (`QualityToolsPage`).
 * - On the fly: the edited PHP file is checked after a pause in typing (tools that can read
 *   unsaved text), when it opens and after it is saved (all tools whose inspection is on).
 * - Problems go to the editor and the Problems tool window (`problems.ts`); Alt+Enter offers
 *   "Fix with PHP CS Fixer / phpcbf / Pint" and Rector's preview on their problems.
 * - Code | Reformat with PHP CS Fixer / phpcbf / Pint, Run Rector on File…, and Inspect Code
 *   with Quality Tools (`inspectWithQualityTools`, the provider an Inspect Code framework
 *   can call for a scope).
 *
 * Rust runs the tools and parses their reports (`commands/php_quality_tools.rs`,
 * `phpforge-quality-tools`); Actions on Save run the fixers through `forge-save-actions`.
 */
import { createEffect, createRoot, on } from "solid-js";
import { ViewPlugin, type ViewUpdate } from "@codemirror/view";
import type { ContextAction } from "@/ipc/codeintel";
import type { QualityTool } from "@/ipc/qualityTools";
import { registerEditorExtension } from "@/editor/extensions";
import { registerContextActionHandler, registerContextActionProvider } from "@/features/codeintel/intentions";
import { action, registerMainMenuEntries, separator } from "@/features/mainMenu/model";
import { registerSettingsPage } from "@/features/settingsPages";
import { registerCommand } from "@/state/commands";
import { activeTab, editor, onFileSaved } from "@/state/editor";
import { getSetting } from "@/state/settings";
import { safeMode } from "@/features/trust/store";
import { viewFor } from "@/editor/views";
import { isPluginActive } from "@/state/project";
import { registerDialog } from "@/shell/window/dialogs";
import { clearAll, problemsOf } from "./problems";
import { QualityToolsPage } from "./QualityToolsPage";
import { RECTOR_DIALOG, RectorPreviewDialog, type RectorPreviewProps } from "./RectorPreviewDialog";
import { active, ALL_TOOLS, checkFile, enabledTools, FIXERS, fixWith, inspectWithQualityTools, isPhpCode, onTheFly } from "./run";
import "./phpQualityTools.css";
import { t } from "@/i18n";

export { inspectWithQualityTools } from "./run";

registerSettingsPage({
  id: "lang.php.quality-tools",
  title: t("settings.page.lang.php.quality-tools"),
  group: "Languages & Frameworks",
  parent: "lang.php",
  order: 1,
  keywords: [
    "phpstan",
    "psalm",
    "php cs fixer",
    "php-cs-fixer",
    "php_codesniffer",
    "phpcs",
    "phpcbf",
    "php mess detector",
    "phpmd",
    "laravel pint",
    "rector",
    "code style",
    "static analysis",
  ],
  component: QualityToolsPage,
});

registerDialog<RectorPreviewProps, { apply: boolean }>({
  id: RECTOR_DIALOG,
  title: "Rector: Preview Changes",
  width: 760,
  height: 540,
  minWidth: 480,
  minHeight: 320,
  component: RectorPreviewDialog,
});

// ---- On the fly --------------------------------------------------------------------------

/** Pause after the last keystroke before the tools check the text. */
const DELAY_MS = 1200;

registerEditorExtension(({ path }) => {
  if (!isPhpCode(path)) return null;
  return ViewPlugin.define((view) => {
    let timer: ReturnType<typeof setTimeout> | undefined;
    // The file as it is on disk when it opens.
    const opened = setTimeout(() => void checkFile(path, null), 300);
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
  ALL_TOOLS.map((t) =>
    ["inspection", "interpreter", "path", "config", "level", "options", "severity"].map((o) => String(getSetting(`php.tools.${t}.${o}`, ""))).join(","),
  ).join("|") + `|${safeMode()}`;

/** After the tools' settings changed (or Safe Mode): forget switched-off tools, check the open PHP files again. */
function recheckOpenFiles(): void {
  const on = new Set(enabledTools());
  clearAll(active() ? ALL_TOOLS.filter((t) => !on.has(t)) : undefined);
  if (!active() || !on.size) return;
  for (const tab of editor.tabs) {
    if (isPhpCode(tab.path) && viewFor(tab.path)) void checkFile(tab.path, viewFor(tab.path)!.state.doc.toString());
  }
}

if (typeof document !== "undefined") {
  createRoot(() => createEffect(on(configSignature, recheckOpenFiles, { defer: true })));
  onFileSaved((path) => {
    // After the Actions on Save tools (Pint, PHP CS Fixer) rewrote the file.
    if (isPhpCode(path)) setTimeout(() => void checkFile(path, null), 200);
  });
}

// ---- Alt+Enter ---------------------------------------------------------------------------

const FIX_TITLES: Partial<Record<QualityTool, string>> = {
  phpCsFixer: "Fix with PHP CS Fixer",
  phpcs: "Fix with phpcbf",
  pint: "Fix with Pint",
  rector: "Preview and apply Rector changes…",
};

registerContextActionProvider(async (path, view) => {
  if (!active() || !isPhpCode(path)) return [];
  const line = view.state.doc.lineAt(view.state.selection.main.head).number - 1;
  const tools = new Set<QualityTool>();
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
        source: "php-quality-tools",
        isPreferred: tool !== "rector",
        needsResolve: false,
        action: "PhpQualityTools.Fix",
        data: { tool },
      }),
    );
});

registerContextActionHandler("PhpQualityTools.Fix", (data, ctx) => fixWith((data as { tool: QualityTool }).tool, ctx.path));

// ---- Code menu ---------------------------------------------------------------------------

const ids: Record<string, string> = {
  phpCsFixer: "PhpQualityTools.ReformatWithPhpCsFixer",
  phpcs: "PhpQualityTools.ReformatWithPhpcbf",
  pint: "PhpQualityTools.ReformatWithPint",
  rector: "PhpQualityTools.RunRector",
};

for (const f of FIXERS) {
  registerCommand({
    id: ids[f.tool],
    title: f.tool === "rector" ? "Run Rector on File…" : `Reformat with ${f.label}`,
    category: "Code",
    run: () => {
      const tab = activeTab();
      if (tab && isPhpCode(tab.path)) void fixWith(f.tool, tab.path);
    },
  });
}

registerCommand({
  id: "PhpQualityTools.InspectCode",
  title: "Inspect Code with Quality Tools",
  category: "Code",
  run: () => void inspectWithQualityTools(null),
});

registerCommand({
  id: "PhpQualityTools.CheckFile",
  title: "Run Quality Tools on Current File",
  category: "Code",
  run: () => {
    const tab = activeTab();
    if (tab && isPhpCode(tab.path) && enabledTools().length) void checkFile(tab.path, tab.content);
  },
});

registerMainMenuEntries(
  "_Code",
  [
    separator,
    action(ids.phpCsFixer, "Reformat with PHP CS _Fixer"),
    action(ids.phpcs, "Reformat with phpc_bf"),
    action(ids.pint, "Reformat with Pi_nt"),
    action(ids.rector, "Run _Rector on File…"),
    action("PhpQualityTools.InspectCode", "Inspect Code with _Quality Tools"),
  ],
  { when: () => isPluginActive("forge.php") },
);
