/**
 * Runs the Actions on Save (Settings | Tools | Actions on Save) for `saveFile`: Rust plans
 * which actions apply; Reformat Code goes through the usual formatter (language server or
 * a pack's formatter) and, for "Changed lines", Rust keeps only the changed lines; the PHP
 * actions run in Rust on the text; the external tools run in Rust on the written file.
 */
import { formattedText } from "@/features/lsp/format";
import { openSettings } from "@/features/settings/dialogState";
import { viewFor } from "@/editor/views";
import { saveActionsApi, isEmptyPlan, type SaveTool, type SaveTrigger, type ToolOutcome } from "@/ipc/saveActions";
import { t } from "@/i18n";
import { balloonsFor } from "@/keymap";
import { isPluginActive } from "@/state/project";
import type { SaveActionsRunner } from "@/state/editor";

const balloon = balloonsFor("Actions on Save");

/** The settings page that configures a tool ("Configure…"). */
export const TOOL_PAGES: Record<SaveTool, string> = {
  eslint: "lang.web.quality-tools",
  prettier: "lang.web.quality-tools",
  stylelint: "lang.web.quality-tools",
  biome: "lang.web.quality-tools",
  phpCsFixer: "lang.php.quality-tools",
  pint: "lang.php.quality-tools",
  phpcbf: "lang.php.quality-tools",
  rector: "lang.php.quality-tools",
};

const TOOL_PLUGINS: Record<SaveTool, string> = {
  eslint: "forge.web",
  prettier: "forge.web",
  stylelint: "forge.web",
  biome: "forge.web",
  phpCsFixer: "forge.php",
  pint: "forge.php",
  phpcbf: "forge.php",
  rector: "forge.php",
};

/** Missing tools are reported once per session, not on every save. */
const reportedMissing = new Set<SaveTool>();

/** Balloons for failed and missing tools (exported for tests). */
export function reportOutcomes(outcomes: ToolOutcome[]): void {
  for (const o of outcomes) {
    const configure = [{ label: t("common.configureEllipsis"), run: () => openSettings(TOOL_PAGES[o.tool]) }];
    if (o.status === "failed") {
      balloon(t("tools.actionsOnSave.toolFailed", { tool: o.label }), o.message, 8000, configure, { type: "error" });
    } else if (o.status === "missing" && !reportedMissing.has(o.tool)) {
      reportedMissing.add(o.tool);
      balloon(t("tools.actionsOnSave.toolSkipped", { tool: o.label }), o.message, 6000, configure, { type: "warning" });
    }
  }
}

export const saveActionsRunner: SaveActionsRunner = {
  async before(path, text, trigger: SaveTrigger) {
    const plan = await saveActionsApi.plan(path, trigger);
    if (isEmptyPlan(plan)) return null;
    let out = text;
    const view = viewFor(path);
    if (plan.reformat && view && view.state.doc.toString() === text) {
      const formatted = await formattedText(path, view);
      out = plan.reformat === "changed" ? await saveActionsApi.changedLines(path, text, formatted) : formatted;
    }
    if ((plan.optimizeImports || plan.rearrange || plan.codeCleanup) && isPluginActive("forge.php")) {
      const result = await saveActionsApi.php(out, {
        optimizeImports: plan.optimizeImports,
        rearrange: plan.rearrange,
        codeCleanup: plan.codeCleanup,
      });
      out = result.text;
    }
    const tools = plan.tools.filter((t) => isPluginActive(TOOL_PLUGINS[t]));
    return { text: out, tools };
  },
  async after(path, tools) {
    try {
      const report = await saveActionsApi.runTools(path, tools as SaveTool[]);
      reportOutcomes(report.outcomes);
      return report.changed;
    } catch (e) {
      balloon(t("tools.actionsOnSave.actionsOnSaveFailed"), String(e instanceof Error ? e.message : e), 8000, undefined, { type: "error" });
      return false;
    }
  },
};
