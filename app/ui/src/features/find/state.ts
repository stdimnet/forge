/** Find in Files popup state (search runs in Rust: `find_in_files`). */
import { createSignal } from "solid-js";
import { activeSelection } from "@/editor/views";

export type FindScope = "project" | "directory";

export interface FindOptions {
  caseSensitive: boolean;
  wholeWords: boolean;
  regex: boolean;
  /** The file mask applies only while checked, like PHPStorm's "File mask" checkbox. */
  maskEnabled: boolean;
  fileMask: string;
  scope: FindScope;
  directory: string;
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
