/** Find in Files popup state (search runs in Rust: `find_in_files`). */
import { createSignal } from "solid-js";
import { activeSelection } from "@/editor/views";
import { findApi, type FindRequest, type FindResult } from "@/ipc/find";
import type { UsagesAnswer } from "@/ipc/codeintel";
import { openInFindWindow } from "@/features/codeintel/usages";
import { scopeRequest } from "./scope";

export type FindScope = "project" | "directory" | "openFiles" | "changedFiles" | "scope";

export interface FindOptions {
  caseSensitive: boolean;
  wholeWords: boolean;
  regex: boolean;
  /** The file mask applies only while checked, like PHPStorm's "File mask" checkbox. */
  maskEnabled: boolean;
  fileMask: string;
  scope: FindScope;
  directory: string;
  /** The scope of the "Scope" tab (a predefined or named scope). */
  scopeName: string;
}

const [findOpen, setFindOpen] = createSignal(false);
const [findQuery, setFindQuery] = createSignal("");
const [findOptions, setFindOptions] = createSignal<FindOptions>({
  caseSensitive: false,
  wholeWords: false,
  regex: false,
  maskEnabled: false,
  fileMask: "*.php",
  scope: "project",
  directory: "",
  scopeName: "Project Files",
});

export { findOpen, findQuery, setFindQuery, findOptions };

export const updateFindOptions = (patch: Partial<FindOptions>) => setFindOptions((o) => ({ ...o, ...patch }));

/**
 * Ctrl+Shift+F: start with the editor selection (a single line), like PHPStorm. From the
 * Project view, `directory` selects the "Directory" scope.
 */
export function openFindInFiles(opts: { directory?: string; query?: string } = {}): void {
  const selected = opts.query ?? activeSelection()?.text;
  if (selected && !selected.includes("\n")) setFindQuery(selected);
  if (opts.directory !== undefined) {
    updateFindOptions(opts.directory ? { scope: "directory", directory: opts.directory } : { scope: "project" });
  }
  setFindOpen(true);
}

export const closeFindInFiles = () => setFindOpen(false);

/** The request the popup sends for a query and its options. */
export function findRequest(query: string, o: FindOptions, limit = 1000): FindRequest {
  return {
    query,
    caseSensitive: o.caseSensitive,
    wholeWords: o.wholeWords,
    regex: o.regex,
    fileMask: o.maskEnabled ? o.fileMask : "",
    ...scopeRequest(o),
    limit,
  };
}

/** Find in Files results in the Find tool window's shape (one group, files, lines). */
export function occurrencesAnswer(query: string, r: FindResult): UsagesAnswer {
  const files = r.files.map((f) => ({
    path: f.path,
    usages: f.matches.map((m) => ({
      line: m.line,
      column: m.column,
      endLine: m.line,
      endColumn: m.column + m.length,
      text: m.text,
      ranges: m.ranges,
      kind: "unclassified" as const,
    })),
  }));
  const tree = { total: r.matchCount, fileCount: files.length, groups: [{ label: "Found Occurrences", count: r.matchCount, files }] };
  return { title: query, source: "none", byType: tree, byFile: tree };
}

/** "Open in Find Window": pin Find in Files results as a tab of the Find tool window. */
export function openFindResultsInWindow(query: string, options: FindOptions, result: FindResult): void {
  const o = { ...options };
  const where =
    o.scope === "directory" && o.directory
      ? `Directory ${o.directory}`
      : o.scope === "scope"
        ? `Scope '${o.scopeName}'`
        : o.scope === "openFiles"
          ? "Open Files"
          : o.scope === "changedFiles"
            ? "Changed Files"
            : "Project Files";
  const mask = o.maskEnabled && o.fileMask ? ` with Mask '${o.fileMask}'` : "";
  openInFindWindow(occurrencesAnswer(query, result), {
    kind: "text",
    title: `Occurrences of '${query}'`,
    header: `Occurrences of '${query}' in ${where}${mask}`,
    rerun: async () => occurrencesAnswer(query, await findApi.search(findRequest(query, o, 5000))),
  });
}
