/**
 * The effective format of open files (indent, line separator for new files, on-save
 * cleanup, right margin and guides), resolved in Rust (`forge-text`) from the code style
 * settings, `.editorconfig` files and the file's content. Resolved when a file opens or is
 * reloaded, and again when a setting that affects it changes.
 */
import { createEffect, createRoot, on } from "solid-js";
import { textApi, type FileFormat } from "@/ipc/text";
import { editor, fileSystemFor, setTabFormat, type EditorTab } from "@/state/editor";
import { settingsState } from "@/state/settings";
import { onFsChanged } from "@/state/fsWatch";
import { fileTypeOf } from "./fileTypes";

const resolving = new Map<string, Promise<FileFormat | null>>();

/** Resolve (again) the format of an open file tab. */
export function refreshFormat(path: string): Promise<FileFormat | null> {
  const tab = editor.tabs.find((t) => t.path === path);
  if (!tab || tab.view || fileSystemFor(path)) return Promise.resolve(null);
  const type = fileTypeOf(path, tab.content);
  const request = textApi
    .fileFormat({ path, languageId: type.id, text: tab.content, fileTypeIndent: type.indent })
    .then((format) => {
      if (editor.tabs.some((t) => t.path === path)) setTabFormat(path, format);
      return format;
    })
    .catch((e) => {
      console.warn(`format of ${path} could not be resolved`, e);
      return null;
    })
    .finally(() => resolving.delete(path));
  resolving.set(path, request);
  return request;
}

/** The format of a tab, resolving it first when needed. */
export async function ensureFormat(tab: EditorTab): Promise<FileFormat | null> {
  return tab.format ?? resolving.get(tab.path) ?? refreshFormat(tab.path);
}

/** Setting values that can change a file's format (code style, On Save, EditorConfig). */
const formatSettings = () =>
  JSON.stringify(
    Object.entries(settingsState.values).filter(
      ([k]) => k.includes(".codeStyle.") || k === "editor.stripTrailingSpaces" || k === "editor.ensureNewlineAtEof",
    ),
  );

createRoot(() => {
  // New file tabs.
  createEffect(() => {
    for (const tab of editor.tabs) {
      if (!tab.format && !tab.view && !resolving.has(tab.path) && !fileSystemFor(tab.path)) void refreshFormat(tab.path);
    }
  });
  // Settings changes apply to every open file.
  createEffect(
    on(
      formatSettings,
      () => {
        for (const tab of editor.tabs) if (tab.format) void refreshFormat(tab.path);
      },
      { defer: true },
    ),
  );
});

// An edited `.editorconfig` changes the files below it.
onFsChanged((changes) => {
  if ([...changes.changed, ...changes.removed].some((p) => p === ".editorconfig" || p.endsWith("/.editorconfig"))) {
    for (const tab of editor.tabs) if (tab.format) void refreshFormat(tab.path);
  }
});
