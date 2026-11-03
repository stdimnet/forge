/**
 * A guest's dropped connection (S6): the host keeps the guest's place for two minutes while
 * the guest's backend tries again with backoff (`forge_collab::reconnect`). This module shows
 * it: a balloon "Connection to <host> lost" with **Leave Session** that stays while Forge is
 * "Reconnecting…", the status item's state, and, once the connection is back, the shared
 * documents brought up to date (edits made meanwhile are sent then) and the views that follow
 * file changes caught up (`catchUpAfterResume`). When reconnecting gives up, the session ends
 * with the backend's message (`collab://ended`).
 */
import { createEffect, createRoot, on } from "solid-js";
import { collabApi, type ConnectionState } from "@/ipc/collab";
import { dismissBalloon, showBalloon } from "@/keymap";
import { dispatchFsChanges, type FsChanges } from "@/state/fsWatch";
import { refreshTree } from "@/features/project/ProjectView";
import { resyncAll } from "./docs";
import { isGuest, session } from "./store";

/** "Reconnecting…" with the next attempt, as the status item and the balloon say it. */
export function connectionLabel(c: ConnectionState | undefined): string | null {
  if (!c || c.state === "connected") return null;
  return c.nextRetrySecs > 0 ? `Reconnecting… (next attempt in ${c.nextRetrySecs} s)` : "Reconnecting…";
}

export const isReconnecting = (): boolean => isGuest() && session().connection?.state === "reconnecting";

/**
 * After a resume: the host sends the file changes it saw while the guest was away
 * (`FsChanged`), but changes queued on the dead connection before the host noticed the drop are
 * lost. So every folder the Project view has listed is read again (expansion and selection
 * stay), and the views that follow file events (Git status, Find, bookmarks, …) refresh as
 * after "Synchronize".
 */
export function catchUpAfterResume(
  views: { refreshTree: () => void; dispatch: (c: FsChanges) => void } = { refreshTree, dispatch: dispatchFsChanges },
): void {
  views.refreshTree();
  views.dispatch({ changed: [], removed: [], git: true });
}

let balloon: number | undefined;

function hideBalloon(): void {
  if (balloon !== undefined) dismissBalloon(balloon);
  balloon = undefined;
}

let tracking = false;

export function trackReconnects(): void {
  if (tracking) return;
  tracking = true;
  void collabApi.onResumed(() => {
    if (!isGuest()) return;
    void resyncAll();
    catchUpAfterResume();
  });
  createRoot(() => {
    createEffect(
      on(isReconnecting, (now, before) => {
        const host = session().hostLabel ?? "the host";
        if (now && !before) {
          hideBalloon();
          balloon = showBalloon(
            `Connection to ${host} lost`,
            "Reconnecting… Your edits are kept and sent when the connection is back.",
            0,
            [{ label: "Leave Session", run: () => void collabApi.leave().catch(() => undefined) }],
            { log: false },
          );
        } else if (!now && before) {
          hideBalloon();
          if (isGuest()) showBalloon("Code With Me", `Reconnected to ${host}.`, 4000);
        }
      }),
    );
  });
}
