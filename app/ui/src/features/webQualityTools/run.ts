/**
 * Running the web quality tools from the editor: on the fly (after a pause in typing, on
 * the editor text), when a file opens and after it is saved; Inspect Code; the fixers
 * (Alt+Enter "Fix ESLint problems", Code | Fix … Problems) and Reformat with Prettier.
 * Rust decides which tools apply to a file, runs them and parses their reports
 * (`web_quality_tools_*`); this file only schedules them and shows the results.
 */
import { WEB_TOOL_LABELS, webQualityToolsApi, type WebCheckResult, type WebFixResult, type WebTool, type WebToolOutcome } from "@/ipc/webQualityTools";
import { replaceChangedPart } from "@/features/lsp/format";
import { openSettings } from "@/features/settings/dialogState";
import { safeMode } from "@/features/trust/store";
import { viewFor } from "@/editor/views";
import { balloonsFor } from "@/keymap/dispatch";
import { showToolWindow } from "@/state/layout";
import { isPluginActive } from "@/state/project";
import { getSetting } from "@/state/settings";
import { applyResult } from "./problems";

const balloon = balloonsFor("Code Quality Tools");
export const PAGE = "lang.web.quality-tools";

export const ALL_TOOLS: WebTool[] = ["eslint", "prettier", "stylelint", "biome"];
export const LINTERS: WebTool[] = ["eslint", "stylelint", "biome"];

/** Files some web tool may handle (Rust decides with the tools' patterns). */
export const isWebFile = (path: string): boolean =>
  /\.(m?[jt]sx?|c[jt]s|vue|html?|css|scss|less|jsonc?|astro|svelte)$/i.test(path) && !/(^|\/)node_modules\//.test(path);

/** Tools that are not disabled. */
export const enabledTools = (tools: WebTool[] = ALL_TOOLS): WebTool[] =>
  tools.filter((t) => getSetting<string>(`web.${t}.mode`, "automatic") !== "disabled");

export const active = (): boolean => isPluginActive("forge.web") && !safeMode();

/** On the fly: after a pause in typing. */
export const onTheFly = (): boolean => getSetting<boolean>("web.tools.onTheFly", true);

const configure = [{ label: "Configure…", run: () => openSettings(PAGE) }];

/** Missing tools and failures are reported once per tool and session, not on every run. */
const reported = new Set<string>();

export function reportOutcomes(outcomes: WebToolOutcome[], always = false): void {
  for (const o of outcomes) {
    if (o.status === "ok") {
      reported.delete(`${o.tool}:failed`);
      continue;
    }
    const key = `${o.tool}:${o.status}`;
    if (!always && reported.has(key)) continue;
    reported.add(key);
    if (o.status === "missing") balloon(`${o.label} was skipped`, o.message, 6000, configure, { type: "warning" });
    else balloon(`${o.label} failed`, o.message, 8000, configure, { type: "error" });
  }
}

const seq = new Map<string, number>();

/**
 * Check `path` with the editor `text` (or the saved file). The result is dropped when the
 * editor text changed meanwhile or a newer check started.
 */
export async function checkFile(path: string, text: string | null): Promise<void> {
  if (!active() || !isWebFile(path) || !enabledTools(LINTERS).length) return;
  const n = (seq.get(path) ?? 0) + 1;
  seq.set(path, n);
  const before = viewFor(path)?.state.doc.toString();
  let result: WebCheckResult;
  try {
    result = await webQualityToolsApi.check(path, text);
  } catch (e) {
    reportOutcomes([{ tool: "eslint", label: "Code quality tools", status: "failed", message: String(e).replace(/^Error: /, "") }]);
    return;
  }
  if (seq.get(path) !== n) return;
  if (before !== undefined && viewFor(path)?.state.doc.toString() !== before) return;
  reportOutcomes(result.outcomes);
  applyResult(result);
}

/** Code | Inspect Code with ESLint, Stylelint and Biome (`paths`: files or folders, null: the project). */
export async function inspectWithWebTools(paths: string[] | null = null): Promise<WebCheckResult | null> {
  if (!isPluginActive("forge.web")) return null;
  if (safeMode()) {
    balloon("Code quality tools are not run in Safe Mode", "They run the project's own code. Trust the project to run them.", 6000, undefined, {
      type: "warning",
    });
    return null;
  }
  const tools = enabledTools(LINTERS);
  if (!tools.length) {
    balloon("No code quality tool is on", "Turn on ESLint, Stylelint or Biome in Settings | Languages & Frameworks | JavaScript | Code Quality Tools.", 6000, configure);
    return null;
  }
  balloon("Inspecting code…", tools.map((t) => WEB_TOOL_LABELS[t]).join(", "), 3000);
  try {
    const result = await webQualityToolsApi.inspect(paths);
    applyResult(result);
    reportOutcomes(result.outcomes, true);
    const files = new Set(result.problems.map((p) => p.file)).size;
    balloon(
      "Code quality tools finished",
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

/** "Fix ESLint problems", "Reformat with Prettier", …: the tool's text, applied as one edit. */
export async function fixWith(tool: WebTool, path: string): Promise<void> {
  const label = WEB_TOOL_LABELS[tool];
  if (!active()) {
    if (safeMode()) balloon("Code quality tools are not run in Safe Mode", "They run the project's own code.", 5000, undefined, { type: "warning" });
    return;
  }
  const view = viewFor(path);
  if (!view) return;
  const text = view.state.doc.toString();
  let result: WebFixResult;
  try {
    result = await webQualityToolsApi.fix(path, tool, text);
  } catch (e) {
    balloon(`${label} failed`, String(e).replace(/^Error: /, ""), 8000, configure, { type: "error" });
    return;
  }
  if (!result.changed) {
    balloon(`${label}: nothing to fix`, path.split("/").pop() ?? path, 3000);
    return;
  }
  if (view.state.doc.toString() !== text) {
    balloon(`${label}: the file changed meanwhile`, "Run it again.", 5000, undefined, { type: "warning" });
    return;
  }
  replaceChangedPart(view, result.text, "input.fix");
  if (tool !== "prettier") void checkFile(path, view.state.doc.toString());
}

/** Reformat Code: Prettier or Biome when turned on for the file, else null (the usual formatter). */
export async function formatWithWebTools(path: string, text: string): Promise<string | null> {
  if (!active() || !isWebFile(path)) return null;
  if (!enabledTools(["prettier", "biome"]).some((t) => getSetting<boolean>(`web.${t}.onReformat`, false))) return null;
  try {
    const result = await webQualityToolsApi.format(path, text);
    return result ? result.text : null;
  } catch (e) {
    balloon("Reformat Code failed", String(e).replace(/^Error: /, ""), 8000, configure, { type: "error" });
    return text;
  }
}
