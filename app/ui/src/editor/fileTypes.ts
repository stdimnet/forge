/**
 * File types of open files, resolved by Rust (`crates/forge-filetypes`): language id, status
 * bar label, indentation, template data language and delimiters, and soft wrapping.
 *
 * Results are cached per path (and `#!` or modeline first line), so the editor can ask synchronously when it
 * is created. The desktop app resolves through IPC: call `preloadFileTypes` (or await
 * `ensureFileType`) before building an editor; a cache miss answers "plain text" and
 * resolves in the background, after which reactive readers update. Browser mode resolves
 * synchronously through the WebAssembly build (see `mock/filetypesMock.ts`).
 */
import { createEffect, createRoot, createSignal, on } from "solid-js";
import { filetypesApi, type DetectRequest, type FileRef, type FileTypeInfo } from "@/ipc/filetypes";
import type { SettingObject } from "@/ipc/settings";
import { getSetting, settingsState } from "@/state/settings";
import { isPluginActive } from "@/state/project";

export type { FileTypeInfo } from "@/ipc/filetypes";
export { PLAIN_TEXT_ID } from "@/ipc/filetypes";

export const PLAIN_TEXT: FileTypeInfo = { id: null, label: "Plain text", indent: "    ", dataLanguage: null, softWrap: false, delimiters: null };

type SyncResolver = (request: DetectRequest) => FileTypeInfo[];
let syncResolver: SyncResolver | null = null;

/** Browser mode: answer cache misses synchronously (the WebAssembly build). */
export function setSyncFileTypeResolver(resolver: SyncResolver | null): void {
  syncResolver = resolver;
  invalidate();
}

const cache = new Map<string, FileTypeInfo>();
const pending = new Map<string, Promise<void>>();
const [revision, setRevision] = createSignal(0);

function invalidate(): void {
  cache.clear();
  setRevision((r) => r + 1);
}

/** Everything besides the files that the answer depends on. */
function config(): Omit<DetectRequest, "files"> {
  return {
    inactivePlugins: settingsState.plugins.filter((p) => !isPluginActive(p.id)).map((p) => p.id),
    dataLanguages: getSetting<string[]>("templates.dataLanguages", []),
    softWrapFiles: getSetting<boolean>("editor.softWraps", false) ? getSetting<string>("editor.softWrapFiles", "") : null,
    templateDelimiters: getSetting<SettingObject[]>("templates.delimiters", []),
    associations: getSetting<SettingObject[]>("fileTypes.associations", []),
    customTypes: getSetting<SettingObject[]>("fileTypes.custom", []),
    overrides: getSetting<SettingObject[]>("fileTypes.overrides", []),
  };
}

/** The settings part of a request (Settings | Editor | File Types lists the types with it). */
export const fileTypeConfig = config;

// Plugin activation (project detection, Settings > Plugins) and file type settings (data
// languages, soft wraps, template delimiters, patterns and custom types) change the answers.
createRoot(() => createEffect(on(() => JSON.stringify(config()), invalidate, { defer: true })));

const firstLineOf = (content?: string) => content?.slice(0, content.search(/\r?\n|$/));

/**
 * Only a `#!` line or a Vim/Emacs modeline can change the answer (Rust decides what it
 * means), so other first lines share one cache entry.
 */
const isTypeLine = (line: string) => line.startsWith("#!") || line.includes("-*-") || /(^|[\s#/;*"!%-])(vim?|ex):/i.test(line);

const typeLineOf = (content?: string): string | undefined => {
  const line = firstLineOf(content);
  return line !== undefined && isTypeLine(line) ? line : undefined;
};

/**
 * The type line of each open file's current text, so readers that only know the path
 * (`languageIdFor(path)`: editor features, structure, LSP) see what the editor sees.
 */
const [typeLines, setTypeLines] = createSignal<ReadonlyMap<string, string>>(new Map());

/**
 * Tell file type detection the current text of an open file. Changes of its first line that
 * matter (a `#!` line typed or pasted into an extensionless file) re-detect its type.
 */
export function noteContent(path: string, content: string): void {
  const line = typeLineOf(content);
  if (typeLines().get(path) === line) return;
  setTypeLines((prev) => {
    const next = new Map(prev);
    if (line === undefined) next.delete(path);
    else next.set(path, line);
    return next;
  });
}

function fileRef(path: string, content?: string): FileRef {
  const firstLine = content === undefined ? typeLines().get(path) : typeLineOf(content);
  return firstLine !== undefined ? { path, firstLine } : { path };
}

const keyOf = (ref: FileRef) => `${ref.path}\n${ref.firstLine ?? ""}`;

/** The file type of `path` (reactive: re-evaluates when a background lookup lands). */
export function fileTypeOf(path: string, content?: string): FileTypeInfo {
  revision();
  const ref = fileRef(path, content);
  const key = keyOf(ref);
  const hit = cache.get(key);
  if (hit) return hit;
  if (syncResolver) {
    const [info] = syncResolver({ files: [ref], ...config() });
    cache.set(key, info);
    return info;
  }
  void preloadFileTypes([{ path, content }]);
  return PLAIN_TEXT;
}

/** Whether `fileTypeOf` answers from a resolved result (not the provisional plain text). */
export function isFileTypeResolved(path: string, content?: string): boolean {
  return !!syncResolver || cache.has(keyOf(fileRef(path, content)));
}

/** Resolve several files in one request (e.g. before opening editors or a diff). */
export async function preloadFileTypes(files: { path: string; content?: string }[]): Promise<void> {
  if (syncResolver) {
    for (const f of files) fileTypeOf(f.path, f.content);
    return;
  }
  const refs = files.map((f) => fileRef(f.path, f.content));
  const waits = refs.map((r) => pending.get(keyOf(r))).filter((p): p is Promise<void> => !!p);
  const missing = refs.filter((r, i) => !cache.has(keyOf(r)) && !pending.has(keyOf(r)) && refs.findIndex((x) => keyOf(x) === keyOf(r)) === i);
  if (missing.length) {
    const request = filetypesApi
      .detect({ files: missing, ...config() })
      .then((infos) => {
        infos.forEach((info, i) => cache.set(keyOf(missing[i]), info));
        setRevision((r) => r + 1);
      })
      .catch((err) => console.warn("file types could not be resolved", err))
      .finally(() => missing.forEach((r) => pending.delete(keyOf(r))));
    missing.forEach((r) => pending.set(keyOf(r), request));
    waits.push(request);
  }
  await Promise.all(waits);
}

/** Resolve one file (no-op when cached). */
export const ensureFileType = (path: string, content?: string) => preloadFileTypes([{ path, content }]);
