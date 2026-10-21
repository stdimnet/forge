/**
 * Code With Me in the editor: which tabs are shared, how they open and save, and what
 * happens when a session starts or ends.
 *
 * - Host: every project file open in the editor is shared (the host refuses excluded files,
 *   which stay normal tabs). Ending the session turns the tabs back into normal tabs.
 * - Guest: tabs `cwm://<path>` show the host's shared files; they close when the session
 *   ends. The guest's own project stays open.
 */
import { createEffect, createMemo, createRoot, on } from "solid-js";
import { collabApi, type Participant, type SessionView } from "@/ipc/collab";
import { workspaceApi } from "@/ipc/workspace";
import { showBalloon } from "@/keymap";
import {
  closeTab,
  editor,
  markTabShared,
  normalizeEol,
  onTabClosed,
  openFile,
  registerFileSystem,
  setSavedHash,
  setSharedTabSource,
  unshareTab,
} from "@/state/editor";
import { workspace } from "@/state/workspace";
import { adoptDoc, GUEST_PREFIX, handleDocEvent, onSharedDocEvent, releaseAll, releaseDoc, retabDoc, sharedDoc } from "./docs";
import { syncBinding } from "./binding";
import { session } from "./store";

/** Project files (not scratches, consoles or other pseudo paths). */
const isProjectFile = (path: string) => !path.includes("://") && !path.startsWith("forge:");

const myself = (s: SessionView): Participant | undefined => s.participants.find((p) => p.id === s.me);

export const guestPath = (path: string) => GUEST_PREFIX + path;

/** Open one of the host's shared files (guest). */
export const openSharedFile = (path: string) => openFile(guestPath(path));

setSharedTabSource({
  open: async (path) => {
    const s = session();
    if (path.startsWith(GUEST_PREFIX)) {
      if (s.role !== "guest") throw new Error("The Code With Me session has ended.");
      const doc = sharedDoc(path) ?? adoptDoc(path, await collabApi.docOpen(path.slice(GUEST_PREFIX.length)), myself(s));
      return { content: doc.ytext.toString(), savedHash: doc.savedHash };
    }
    if (s.role !== "host" || !isProjectFile(path)) return undefined;
    try {
      const doc = sharedDoc(path) ?? adoptDoc(path, await collabApi.docOpen(path), myself(s));
      return { content: doc.ytext.toString(), savedHash: doc.savedHash };
    } catch {
      return undefined; // not shared (excluded, too large): a normal tab
    }
  },
  save: async (path) => {
    const doc = sharedDoc(path);
    if (!doc) return;
    try {
      await collabApi.docSave(doc.docId);
    } catch (e) {
      showBalloon("Cannot save", String((e as Error)?.message ?? e));
    }
  },
});

// Guests' tabs: breadcrumbs under "Code With Me"; reading and writing go through the session.
registerFileSystem({
  prefix: GUEST_PREFIX,
  read: () => Promise.reject(new Error("Shared files open through the Code With Me session.")),
  write: () => Promise.resolve(),
  breadcrumbs: (path) => ["Code With Me", ...path.slice(GUEST_PREFIX.length).split("/")],
});

onTabClosed((tab) => releaseDoc(tab.path));

onSharedDocEvent((doc, e) => {
  if (e.saved) setSavedHash(doc.tabPath, e.saved.hash);
  if (e.renamed !== undefined && doc.tabPath.startsWith(GUEST_PREFIX)) {
    // The host renamed the file: the guest's tab follows (the host's own UI reopens it).
    const old = doc.tabPath;
    retabDoc(doc, guestPath(e.renamed));
    closeTab(old);
    void openFile(doc.tabPath);
  }
  if (e.closed !== undefined) {
    const path = doc.tabPath;
    releaseDoc(path, false);
    if (path.startsWith(GUEST_PREFIX)) {
      showBalloon("Code With Me", e.closed);
      closeTab(path);
    } else {
      unshareTab(path, "");
      syncBinding(path);
    }
  }
});

/** Share the project files already open in the editor (the session just started). */
async function shareOpenTabs(s: SessionView): Promise<void> {
  for (const tab of editor.tabs) {
    if (tab.view || tab.shared || !isProjectFile(tab.path)) continue;
    try {
      const doc = sharedDoc(tab.path) ?? adoptDoc(tab.path, await collabApi.docOpen(tab.path, tab.content), myself(s));
      markTabShared(tab.path, doc.savedHash);
      syncBinding(tab.path);
    } catch {
      // excluded or too large: stays a normal tab
    }
  }
}

/** The session ended on the host: shared tabs become normal tabs with their text kept. */
async function unshareTabs(): Promise<void> {
  for (const tab of editor.tabs.filter((t) => t.shared && !t.path.startsWith(GUEST_PREFIX))) {
    releaseDoc(tab.path, false);
    const disk = await workspaceApi
      .readFile(tab.path)
      .then((raw) => normalizeEol(raw).text)
      .catch(() => "");
    unshareTab(tab.path, disk);
    syncBinding(tab.path);
  }
}

function closeGuestTabs(): void {
  for (const tab of editor.tabs.filter((t) => t.path.startsWith(GUEST_PREFIX))) {
    releaseDoc(tab.path, false);
    closeTab(tab.path);
  }
}

/** Follow the session: share on start, open the host's files on join, clean up on end. */
export function trackEditorSharing(): void {
  void collabApi.onDoc(handleDocEvent);
  void collabApi.onNotice((e) => showBalloon("Code With Me", e.text));
  void collabApi.onEnded((e) => showBalloon("Code With Me", e.reason, 6000));
  createRoot(() => {
    // A memo: status events arrive all the time and must not rerun this for the same role.
    const role = createMemo(() => session().role);
    createEffect(
      on(
        role,
        (role, prev) => {
          const s = session();
          if (role === "host") void shareOpenTabs(s);
          if (role === "guest" && prev !== "guest") {
            const files = s.files.slice(0, 10);
            void (async () => {
              for (const f of files) await openSharedFile(f.path).catch((e) => console.warn(`cannot open shared file ${f.path}`, e));
              // The host's first file is the one to look at.
              if (files[0] && sharedDoc(guestPath(files[0].path))) await openSharedFile(files[0].path).catch(() => undefined);
            })();
          }
          if (role !== "host" && prev === "host") void unshareTabs();
          if (role !== "guest" && prev === "guest") closeGuestTabs();
          if (role === "none") releaseAll();
        },
      ),
    );
    // A host that opens another project ends the session.
    createEffect(
      on(
        createMemo(() => workspace()?.root),
        (_root, prev) => {
          if (prev !== undefined && session().role === "host") void collabApi.end();
        },
        { defer: true },
      ),
    );
  });
}
