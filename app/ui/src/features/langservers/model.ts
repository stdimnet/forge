/** Pure helpers for the Language Servers settings page and symbol search. */
import type { RowStatus, ServerRow, SkipReason, WorkspaceSymbol } from "@/ipc/langservers";
import type { SearchItemKind } from "@/features/search/providers";

export const STATUS_LABEL: Record<RowStatus, string> = {
  running: "Running",
  installed: "Installed",
  notInstalled: "Not installed",
  disabled: "Disabled",
};

export interface RowGroup {
  title: string;
  rows: ServerRow[];
}

/** Rows grouped by language heading; groups keep catalog order, project languages first. */
export function groupRows(rows: ServerRow[], opts: { filter?: string; projectOnly?: boolean } = {}): RowGroup[] {
  const q = (opts.filter ?? "").trim().toLowerCase();
  const visible = rows.filter((r) => {
    if (opts.projectOnly && !r.inProject) return false;
    if (!q) return true;
    return [r.name, r.id, r.group, r.command, ...r.languages].some((s) => s.toLowerCase().includes(q));
  });
  const groups = new Map<string, RowGroup>();
  for (const r of visible) {
    const g = groups.get(r.group) ?? { title: r.group, rows: [] };
    g.rows.push(r);
    groups.set(r.group, g);
  }
  const inProject = (g: RowGroup) => (g.rows.some((r) => r.inProject) ? 0 : 1);
  return [...groups.values()].sort((a, b) => inProject(a) - inProject(b));
}

/** Why a server does not run in this project, in words. */
export function skipText(skip: SkipReason | null | undefined, rows: ServerRow[] = []): string | null {
  if (!skip) return null;
  switch (skip.reason) {
    case "autoStartOff":
      return "Automatic start is off.";
    case "disabled":
      return "Disabled.";
    case "pluginDisabled":
      return `The ${skip.plugin} plugin is disabled (Settings | Plugins).`;
    case "notInProject":
      return "Not needed: the project has no files of its languages.";
    case "notUsed":
      return "Not needed: the project has no configuration or dependency for it.";
    case "notInstalled":
      return "Not installed.";
    case "alternative": {
      const used = rows.find((r) => r.id === skip.used)?.name ?? skip.used;
      return `Not started: ${used} is used instead.`;
    }
  }
}

/** `/home/me/.npm-global/bin/x` → `~/.npm-global/bin/x`. */
export function shortenHome(path: string | null | undefined): string {
  if (!path) return "";
  return path.replace(/^(\/home\/[^/]+|\/Users\/[^/]+|[A-Za-z]:\\Users\\[^\\]+)(?=[/\\]|$)/, "~");
}

/** Validate the extra settings JSON as typed: `null` when fine (or empty). */
export function jsonError(text: string): string | null {
  if (!text.trim()) return null;
  try {
    const v = JSON.parse(text);
    if (typeof v !== "object" || v === null || Array.isArray(v)) return "Must be a JSON object.";
    return null;
  } catch (e) {
    return (e as Error).message;
  }
}

/** Search Everywhere badge kind for an LSP symbol kind. */
export function searchKind(kind: string): SearchItemKind | undefined {
  switch (kind) {
    case "class":
    case "struct":
    case "object":
      return "class";
    case "interface":
      return "interface";
    case "enum":
      return "enum";
    case "function":
    case "constructor":
      return "function";
    case "method":
      return "method";
    case "property":
    case "field":
    case "variable":
    case "key":
      return "property";
    case "constant":
    case "enumMember":
      return "constant";
    default:
      return undefined;
  }
}

/** Kinds shown in the Classes tab. */
export const CLASS_KINDS = new Set(["class", "interface", "struct", "enum"]);

/** De-duplicate symbols reported by several servers for the same place. */
export function uniqueSymbols(symbols: WorkspaceSymbol[]): WorkspaceSymbol[] {
  const seen = new Set<string>();
  return symbols.filter((s) => {
    const key = `${s.path}:${s.range?.start.line ?? -1}:${s.name}`;
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}
