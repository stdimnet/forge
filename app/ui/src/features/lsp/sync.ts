/** Keeps the language server's view of a file in sync with the editor (full sync). */
import { ViewPlugin, type ViewUpdate } from "@codemirror/view";
import { lspApi } from "@/ipc/lsp";
import { onFileSaved } from "@/state/editor";
import { fileTypeOf, PLAIN_TEXT_ID } from "@/editor/fileTypes";

/** The file type servers are told: detected or overridden (`text` keeps them all away). */
function languageOf(path: string): string {
  const info = fileTypeOf(path);
  return info.id ?? (info.overridden ? PLAIN_TEXT_ID : "");
}

const CHANGE_DEBOUNCE_MS = 200;

const ignore = () => {};

/** Debounced `didChange` per path, flushed before requests that depend on it. */
const pending = new Map<string, () => Promise<void>>();

/**
 * Re-announce functions of the open editors by path, see `reopenDocuments`. A file shown in
 * several editors (split views) has one per editor; the server sees it as one document, opened
 * with the first editor and closed with the last.
 */
const announcers = new Map<string, Set<() => void>>();

/**
 * Announce every open document again so servers registered since (installed, enabled)
 * attach to files that were opened before they existed.
 */
export function reopenDocuments(): void {
  announcers.forEach((views) => views.values().next().value?.());
}

/** Send any debounced edits of `path` now. Call before position-based requests. */
export function syncNow(path: string): Promise<void> {
  return pending.get(path)?.() ?? Promise.resolve();
}

export function documentSync(path: string) {
  return ViewPlugin.define((view) => {
    let timer: ReturnType<typeof setTimeout> | undefined;
    const flush = async () => {
      if (timer === undefined) return;
      clearTimeout(timer);
      timer = undefined;
      pending.delete(path);
      await lspApi.didChange(path, view.state.doc.toString()).catch(ignore);
    };
    const announce = () => void lspApi.didOpen(path, view.state.doc.toString(), languageOf(path)).catch(ignore);
    const views = announcers.get(path) ?? new Set();
    if (!views.size) announce();
    views.add(announce);
    announcers.set(path, views);
    const offSave = onFileSaved(async (p, content) => {
      if (p !== path) return;
      await flush();
      lspApi.didSave(path, content).catch(ignore);
    });
    return {
      update(u: ViewUpdate) {
        if (!u.docChanged) return;
        clearTimeout(timer);
        timer = setTimeout(() => void flush(), CHANGE_DEBOUNCE_MS);
        pending.set(path, flush);
      },
      destroy() {
        clearTimeout(timer);
        if (pending.get(path) === flush) pending.delete(path);
        offSave();
        views.delete(announce);
        if (views.size) return;
        announcers.delete(path);
        lspApi.didClose(path).catch(ignore);
      },
    };
  });
}
