/**
 * The quality tools' problems in the editor and the Problems tool window: per file and
 * tool, published as the diagnostics source `php-quality-tools`. A check replaces the
 * earlier problems of the tools that ran in the files it checked (all files for Inspect
 * Code on the whole project).
 */
import type { EditorDiagnostic } from "@/ipc/lsp";
import { TOOL_LABELS, type CheckResult, type QualityProblem, type QualityTool } from "@/ipc/qualityTools";
import { publishDiagnostics } from "@/features/lsp/store";
import { viewFor } from "@/editor/views";

export const SOURCE = "php-quality-tools";

/** The data of a quality tool's diagnostic (Alt+Enter fixes). */
export interface QualityDiagnosticData {
  qualityTool: QualityTool;
  fixable: boolean;
  rule?: string;
}

const problems = new Map<string, Map<QualityTool, QualityProblem[]>>();

/** The problems of `path` (all tools). */
export function problemsOf(path: string): QualityProblem[] {
  return [...(problems.get(path)?.values() ?? [])].flat();
}

/** First non-blank column of a line of an open file (where a line problem starts). */
function indentOf(path: string, line: number): number {
  const doc = viewFor(path)?.state.doc;
  if (!doc || line >= doc.lines) return 0;
  const text = doc.line(line + 1).text;
  return text.length - text.trimStart().length;
}

export function toDiagnostic(p: QualityProblem): EditorDiagnostic {
  const whole = p.endLine === undefined && p.endColumn === undefined;
  const start = whole && p.column === 0 ? indentOf(p.file, p.line) : p.column;
  const end = whole
    ? { line: p.line, character: 100000 }
    : { line: p.endLine ?? p.line, character: p.endColumn ?? (p.endLine === undefined ? p.column + 1 : 100000) };
  const data: QualityDiagnosticData = { qualityTool: p.tool, fixable: p.fixable, rule: p.rule };
  return {
    range: { start: { line: p.line, character: start }, end },
    severity: p.severity,
    message: p.link ? `${p.message} (${p.link})` : p.message,
    source: TOOL_LABELS[p.tool],
    code: p.rule ?? null,
    data,
  };
}

function publish(path: string): void {
  publishDiagnostics(path, problemsOf(path).map(toDiagnostic), SOURCE);
}

/** Show a check's problems; returns the files whose problems changed. */
export function applyResult(r: CheckResult): string[] {
  const touched = new Set<string>();
  const clear = (path: string) => {
    const tools = problems.get(path);
    if (!tools) return;
    for (const t of r.tools) if (tools.delete(t)) touched.add(path);
  };
  if (r.files.length) {
    // A folder of Inspect Code covers the files below it.
    for (const path of [...problems.keys()]) {
      if (r.files.some((f) => path === f || path.startsWith(f.endsWith("/") ? f : `${f}/`))) clear(path);
    }
  } else {
    for (const path of [...problems.keys()]) clear(path);
  }
  for (const p of r.problems) {
    let tools = problems.get(p.file);
    if (!tools) problems.set(p.file, (tools = new Map()));
    tools.set(p.tool, [...(tools.get(p.tool) ?? []), p]);
    touched.add(p.file);
  }
  for (const f of r.files) touched.add(f);
  for (const path of touched) publish(path);
  return [...touched];
}

/** Forget every problem (tools switched off, Safe Mode, project closed). */
export function clearAll(tools?: QualityTool[]): void {
  for (const [path, byTool] of problems) {
    for (const t of tools ?? [...byTool.keys()]) byTool.delete(t);
    publish(path);
  }
}
