/**
 * Running the quality tools from the editor: on the fly (after a pause in typing, on the
 * editor text), when a PHP file opens and after it is saved; Inspect Code with the quality
 * tools; the fixers ("Reformat with …", Alt+Enter "Fix with …", Rector's preview). Rust
 * runs the tools (`quality_tools_*`); this file only schedules them and shows the results.
 */
import { qualityToolsApi, TOOL_LABELS, type CheckResult, type FixResult, type QualityTool, type QualityToolOutcome } from "@/ipc/qualityTools";
import { replaceChangedPart } from "@/features/lsp/format";
import { openSettings } from "@/features/settings/dialogState";
import { safeMode } from "@/features/trust/store";
import { viewFor } from "@/editor/views";
import { balloonsFor } from "@/keymap/dispatch";
import { editor, saveFile } from "@/state/editor";
import { showToolWindow } from "@/state/layout";
import { isPluginActive } from "@/state/project";
import { getSetting } from "@/state/settings";
import { openDialogWindow } from "@/shell/window/dialogs";
import { applyResult } from "./problems";
import { RECTOR_DIALOG, type RectorPreviewProps } from "./RectorPreviewDialog";

const balloon = balloonsFor("PHP Quality Tools");
const PAGE = "lang.php.quality-tools";

export const ALL_TOOLS: QualityTool[] = ["phpcs", "phpCsFixer", "phpmd", "phpstan", "psalm", "pint", "rector"];

/** PHP code files (not Blade templates). */
export const isPhpCode = (path: string): boolean => /\.(php|inc|phtml)$/i.test(path) && !path.endsWith(".blade.php");

/** Tools whose inspection is on. */
export const enabledTools = (): QualityTool[] => ALL_TOOLS.filter((t) => getSetting<boolean>(`php.tools.${t}.inspection`, false));

export const active = (): boolean => isPluginActive("forge.php") && !safeMode();

const configure = [{ label: "Configure…", run: () => openSettings(PAGE) }];

/** Missing tools and failures are reported once per tool and session, not on every run. */
const reported = new Set<string>();

export function reportOutcomes(outcomes: QualityToolOutcome[], always = false): void {
  for (const o of outcomes) {
    if (o.status === "ok") continue;
    const key = `${o.tool}:${o.status}`;
    if (!always && reported.has(key)) continue;
    reported.add(key);
    if (o.status === "missing") balloon(`${o.label} was skipped`, o.message, 6000, configure, { type: "warning" });
    else balloon(`${o.label} failed`, o.message, 8000, configure, { type: "error" });
  }
}

/** A tool ran fine again: its next failure is reported. */
function forgetReported(outcomes: QualityToolOutcome[]): void {
  for (const o of outcomes) if (o.status === "ok") reported.delete(`${o.tool}:failed`);
}

const seq = new Map<string, number>();

/**
 * Check `path`: with `text` (the editor text, maybe unsaved) or the saved file. The result
 * is dropped when the editor text changed meanwhile or a newer check started.
 */
export async function checkFile(path: string, text: string | null): Promise<void> {
  if (!active() || !isPhpCode(path) || !enabledTools().length) return;
  const n = (seq.get(path) ?? 0) + 1;
  seq.set(path, n);
  const before = viewFor(path)?.state.doc.toString();
  let result: CheckResult;
  try {
    result = await qualityToolsApi.check(path, text);
  } catch (e) {
    reportOutcomes([{ tool: "phpstan", label: "PHP quality tools", status: "failed", message: String(e).replace(/^Error: /, "") }]);
    return;
  }
  if (seq.get(path) !== n) return;
  if (before !== undefined && viewFor(path)?.state.doc.toString() !== before) return;
  forgetReported(result.outcomes);
  reportOutcomes(result.outcomes);
  applyResult(result);
}

/** On the fly: after a pause in typing. */
export const onTheFly = (): boolean => getSetting<boolean>("php.tools.onTheFly", true);

/** Code | Inspect Code with Quality Tools (`paths`: files or folders, null: the project). */
export async function inspectWithQualityTools(paths: string[] | null = null): Promise<CheckResult | null> {
  if (!isPluginActive("forge.php")) return null;
  if (safeMode()) {
    balloon("Quality tools are not run in Safe Mode", "They run the project's own code. Trust the project to run them.", 6000, undefined, { type: "warning" });
    return null;
  }
  const tools = enabledTools();
  if (!tools.length) {
    balloon("No quality tool is on", "Turn on PHPStan, Psalm, PHP_CodeSniffer, … in Settings | Languages & Frameworks | PHP | Quality Tools.", 6000, configure);
    return null;
  }
  balloon("Inspecting code with quality tools…", tools.map((t) => TOOL_LABELS[t]).join(", "), 3000);
  try {
    const result = await qualityToolsApi.inspect(paths);
    applyResult(result);
    reportOutcomes(result.outcomes, true);
    const files = new Set(result.problems.map((p) => p.file)).size;
    balloon(
      "Quality tools finished",
      result.problems.length ? `${result.problems.length} problems in ${files} files.` : "No problems found.",
      6000,
      result.problems.length ? [{ label: "Show Problems", run: () => showToolWindow("problems") }] : undefined,
    );
    return result;
  } catch (e) {
    balloon("Inspect Code failed", String(e).replace(/^Error: /, ""), 8000, configure, { type: "error" });
    return null;
  }
}

/** Fixers in menus: the tool that fixes and its name. */
export const FIXERS: { tool: QualityTool; label: string }[] = [
  { tool: "phpCsFixer", label: "PHP CS Fixer" },
  { tool: "phpcs", label: "phpcbf" },
  { tool: "pint", label: "Pint" },
  { tool: "rector", label: "Rector" },
];

const fixerLabel = (tool: QualityTool) => FIXERS.find((f) => f.tool === tool)?.label ?? TOOL_LABELS[tool];

/** Apply a fixer's text to the editor as one edit, when nobody typed meanwhile. */
function applyFix(path: string, before: string, result: FixResult): boolean {
  const view = viewFor(path);
  if (!view || view.state.doc.toString() !== before) return false;
  replaceChangedPart(view, result.text, "input.fix");
  return true;
}

/**
 * Fix `path` with `tool` ("Reformat with …", "Fix with …"). Pint and Rector work on the
 * saved file, so it is saved first; Rector's changes are shown before they are applied.
 */
export async function fixWith(tool: QualityTool, path: string): Promise<void> {
  if (!active()) {
    if (safeMode()) balloon("Quality tools are not run in Safe Mode", "They run the project's own code.", 5000, undefined, { type: "warning" });
    return;
  }
  const label = fixerLabel(tool);
  if (tool === "pint" || tool === "rector") {
    const tab = editor.tabs.find((t) => t.path === path);
    if (tab && tab.content !== tab.saved) await saveFile(path, "plain");
  }
  const view = viewFor(path);
  if (!view) return;
  const text = view.state.doc.toString();
  let result: FixResult;
  try {
    result = await qualityToolsApi.fix(path, tool, text);
  } catch (e) {
    balloon(`${label} failed`, String(e).replace(/^Error: /, ""), 8000, configure, { type: "error" });
    return;
  }
  if (!result.changed) {
    balloon(`${label}: nothing to fix`, path.split("/").pop() ?? path, 3000);
    return;
  }
  if (tool === "rector") {
    const props: RectorPreviewProps = { path, diff: result.diff, rules: result.rules };
    const answer = await openDialogWindow<{ apply: boolean }>(RECTOR_DIALOG, props);
    if (!answer?.apply) return;
  }
  if (!applyFix(path, text, result)) {
    balloon(`${label}: the file changed meanwhile`, "Run it again.", 5000, undefined, { type: "warning" });
    return;
  }
  void checkFile(path, viewFor(path)?.state.doc.toString() ?? null);
}
