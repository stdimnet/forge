/** Keeps the language server's view of a file in sync with the editor (full sync). */
import { ViewPlugin, type ViewUpdate } from "@codemirror/view";
import { lspApi } from "@/ipc/lsp";
import { onFileSaved } from "@/state/editor";
import { fileTypeOf } from "@/editor/fileTypes";

/** The file type servers are told (`artisan` is PHP by its `#!` line). */
const languageOf = (path: string): string => fileTypeOf(path).id ?? "";

const CHANGE_DEBOUNCE_MS = 200;

const ignore = () => {};

/** Debounced `didChange` per path, flushed before requests that depend on it. */
const pending = new Map<string, () => Promise<void>>();

/** Re-announce functions of the open editors, see `reopenDocuments`. */
const announcers = new Map<string, () => void>();

/**
 * Announce every open document again so servers registered since (installed, enabled)
 * attach to files that were opened before they existed.
 */
export function reopenDocuments(): void {
  announcers.forEach((announce) => announce());
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
    announce();
    announcers.set(path, announce);
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
        pending.delete(path);
        announcers.delete(path);
        offSave();
        lspApi.didClose(path).catch(ignore);
      },
    };
  });
}
