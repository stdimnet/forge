/**
 * The web quality tools' problems in the editor and the Problems tool window: per file and
 * tool, published as the diagnostics source `web-quality-tools`. A check replaces the
 * earlier problems of the tools it names in the files it checked (all files for Inspect
 * Code on the whole project).
 */
import type { EditorDiagnostic } from "@/ipc/lsp";
import { WEB_TOOL_LABELS, type WebCheckResult, type WebProblem, type WebTool } from "@/ipc/webQualityTools";
import { publishDiagnostics } from "@/features/lsp/store";

export const SOURCE = "web-quality-tools";

/** The data of a web tool's diagnostic (Alt+Enter fixes). */
export interface WebDiagnosticData {
  webTool: WebTool;
  fixable: boolean;
  rule?: string;
}

const problems = new Map<string, Map<WebTool, WebProblem[]>>();

/** The problems of `path` (all tools). */
export function problemsOf(path: string): WebProblem[] {
  return [...(problems.get(path)?.values() ?? [])].flat();
}

/** "ESLint: … (eqeqeq)" like PHPStorm. */
function messageOf(p: WebProblem): string {
  const rule = p.rule ? ` (${p.rule})` : "";
  return `${WEB_TOOL_LABELS[p.tool]}: ${p.message}${rule}`;
}

export function toDiagnostic(p: WebProblem): EditorDiagnostic {
  const end =
    p.endLine !== undefined || p.endColumn !== undefined
      ? { line: p.endLine ?? p.line, character: p.endColumn ?? p.column + 1 }
      : { line: p.line, character: p.column + 1 };
  // An empty range (a zero-width end) still gets one character.
  if (end.line === p.line && end.character <= p.column) end.character = p.column + 1;
  const data: WebDiagnosticData = { webTool: p.tool, fixable: p.fixable, rule: p.rule };
  return {
    range: { start: { line: p.line, character: p.column }, end },
    severity: p.severity,
    message: messageOf(p),
    source: WEB_TOOL_LABELS[p.tool],
    code: p.rule ?? null,
    data,
  };
}

function publish(path: string): void {
  publishDiagnostics(path, problemsOf(path).map(toDiagnostic), SOURCE);
}

/** Show a check's problems; returns the files whose problems changed. */
export function applyResult(r: WebCheckResult): string[] {
  const touched = new Set<string>();
  const clear = (path: string) => {
    const tools = problems.get(path);
    if (!tools) return;
    for (const t of r.tools) if (tools.delete(t)) touched.add(path);
  };
  if (r.files.length) {
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

/** Forget every problem (tools switched off, Safe Mode). */
export function clearAll(): void {
  for (const [path, byTool] of problems) {
    byTool.clear();
    publish(path);
  }
}
