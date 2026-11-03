/**
 * File Cache Conflict for shared files (PHPStorm): a shared file changed on the host's disk
 * (git, a formatter, an AI agent) while the shared document has unsaved changes. The host's
 * backend keeps the document and reports the conflict (`collab://doc` with `conflict`); it
 * decides everything else (what "resolved" means, refusing guests' saves meanwhile).
 *
 * - Host: the File Cache Conflict dialog asks "Load File System Changes" or "Keep Memory
 *   Changes"; "Show Difference" opens the diff and leaves the choice in a sticky balloon, as
 *   does closing the dialog. The answer goes to `collab_doc_resolve_conflict`.
 * - Guest: a balloon says that the host is resolving the conflict; the result arrives as a
 *   normal edit and save state.
 */
import type { DocEvent } from "@/ipc/collab";

export const CONFLICT_TITLE = "File Cache Conflict";
export const LOAD_LABEL = "Load File System Changes";
export const KEEP_LABEL = "Keep Memory Changes";
export const DIFF_LABEL = "Show Difference";

export const fileName = (path: string): string => path.split("/").pop() || path;

export interface ConflictUi {
  role: () => "host" | "guest" | "none";
  /** Ask the host's user; resolves with "load", "keep", "diff" or null (closed). */
  ask: (title: string, message: string, choices: { id: string; label: string }[]) => Promise<string | null>;
  resolve: (docId: number, load: boolean) => Promise<void>;
  /** Show a balloon; `sticky` balloons stay until an action or close. Returns its id. */
  balloon: (title: string, detail: string, actions?: { label: string; run: () => void }[], sticky?: boolean) => number;
  dismiss: (id: number) => void;
  showDiff: (docId: number, path: string) => Promise<void>;
  /** The guest's tab name of a shared document, or the host's name. */
  docName: (docId: number) => string | undefined;
  hostName: () => string;
  error: (text: string) => void;
}

/** The conflict flow of one window; `handle` takes every `collab://doc` event. */
export function conflictHandler(ui: ConflictUi) {
  /** Open conflicts: the path (host) and the balloon showing the choice, if any. */
  const open = new Map<number, { path: string; balloon?: number }>();
  /** Conflicts waiting for the dialog, one dialog at a time. */
  const queue: number[] = [];
  let asking = false;

  const answer = async (docId: number, load: boolean) => {
    const c = open.get(docId);
    if (!c) return;
    if (c.balloon !== undefined) ui.dismiss(c.balloon);
    open.delete(docId);
    // The conflict may have ended meanwhile (the host saved, the disk changed back).
    await ui.resolve(docId, load).catch((e) => ui.error(String((e as Error)?.message ?? e)));
  };

  const offer = (docId: number) => {
    const c = open.get(docId);
    if (!c || c.balloon !== undefined) return;
    c.balloon = ui.balloon(
      CONFLICT_TITLE,
      `Changes have been made to ${fileName(c.path)} in memory and on disk.`,
      [
        { label: LOAD_LABEL, run: () => void answer(docId, true) },
        { label: KEEP_LABEL, run: () => void answer(docId, false) },
      ],
      true,
    );
  };

  const next = async () => {
    if (asking) return;
    const docId = queue.shift();
    if (docId === undefined) return;
    const c = open.get(docId);
    if (!c) return void next();
    asking = true;
    try {
      const id = await ui.ask(CONFLICT_TITLE, `Changes have been made to '${c.path}' in memory and on disk.`, [
        { id: "diff", label: DIFF_LABEL },
        { id: "keep", label: KEEP_LABEL },
        { id: "load", label: LOAD_LABEL },
      ]);
      if (id === "load" || id === "keep") await answer(docId, id === "load");
      else if (open.has(docId)) {
        offer(docId);
        if (id === "diff") await ui.showDiff(docId, c.path).catch((e) => ui.error(String((e as Error)?.message ?? e)));
      }
    } finally {
      asking = false;
    }
    void next();
  };

  return {
    handle(e: DocEvent): void {
      const conflict = e.conflict;
      if (!conflict) return;
      const role = ui.role();
      if (!conflict.open) {
        const c = open.get(e.docId);
        if (c?.balloon !== undefined) ui.dismiss(c.balloon);
        open.delete(e.docId);
        return;
      }
      if (open.has(e.docId)) return;
      if (role === "host") {
        open.set(e.docId, { path: conflict.path });
        queue.push(e.docId);
        void next();
      } else if (role === "guest") {
        const name = ui.docName(e.docId) ?? "A shared file";
        open.set(e.docId, { path: name });
        open.get(e.docId)!.balloon = ui.balloon(
          CONFLICT_TITLE,
          `${name} changed on ${ui.hostName()}'s disk while it has unsaved changes. ${ui.hostName()} is choosing which version to keep; saving waits until then.`,
          undefined,
          false,
        );
      }
    },
    /** Open conflicts (for tests and the session's end). */
    openConflicts: (): number[] => [...open.keys()],
    reset(): void {
      for (const c of open.values()) if (c.balloon !== undefined) ui.dismiss(c.balloon);
      open.clear();
      queue.length = 0;
    },
  };
}
