/**
 * A Code With Me guest's window: the host's project is this window's project (opened by the
 * backend as a `CollabBackend`), the tool windows that make sense for guests, and what the
 * guest's permissions allow in the Project view. The state is the session as the backend
 * reports it (`store.ts`).
 */
import { createEffect, createRoot, on } from "solid-js";
import { closeWorkspace, adoptWorkspace, isCollabGuest } from "@/state/workspace";
import { showBalloon } from "@/keymap";
import { levelOf, type PermissionLevel, type Permissions, collabApi } from "@/ipc/collab";
import { Compartment, EditorState, type Extension } from "@codemirror/state";
import { ViewPlugin, type EditorView } from "@codemirror/view";
import { registerEditorExtension } from "@/editor/extensions";
import { registerToolWindowFilter } from "@/features/registry";
import { session } from "./store";
import { t } from "@/i18n";

/** This guest's permissions (`undefined` unless this window is a guest). */
export const guestPermissions = (): Permissions | undefined => (session().role === "guest" ? session().permissions : undefined);

/** Guests can create, rename, delete and edit files only with Files: Edit. */
export const guestCanEdit = (): boolean => !isCollabGuest() || guestPermissions()?.files === "edit";

export const guestLevel = (): PermissionLevel | undefined => {
  const p = guestPermissions();
  return p ? levelOf(p) : undefined;
};

export const LEVEL_NAMES: Readonly<Record<PermissionLevel, string>> = {
  get readOnly() {
    return t("tools.codeWithMe.level.readOnly");
  },
  get editFiles() {
    return t("tools.codeWithMe.level.editFiles");
  },
  get fullAccess() {
    return t("tools.codeWithMe.level.fullAccess");
  },
  get custom() {
    return t("tools.codeWithMe.level.custom");
  },
};

/**
 * Tool windows a guest does not get. AI Chat, Database, Services and Pull Requests would use
 * the host's secrets and accounts or the guest's own, and the Debug window controls the
 * host's debugger. Terminal and Run show the host's shared terminals and runs, so they need
 * Terminal or Run access; the Commit window writes to the host's repository, so it needs Full
 * access to version control.
 */
export function hiddenForGuest(id: string): boolean {
  if (!isCollabGuest()) return false;
  if (["ai-chat", "database", "services", "debug", "pull-requests"].includes(id)) return true;
  const p = guestPermissions();
  if (id === "terminal") return !p || p.terminal === "none";
  if (id === "run") return !p || p.run === "none";
  if (id === "commit") return p?.vcs !== "full";
  return false;
}

registerToolWindowFilter((tw) => !hiddenForGuest(tw.id));

// A guest without Files: Edit reads the host's files in read-only editors (the host drops
// their changes anyway; this is for usability). The host can change the permission during
// the session, so open editors are reconfigured. Only ever `true`: `EditorState.readOnly`
// takes the first value, and a shared read-only document says `true` itself.
const access = new Compartment();
const guestViews = new Set<EditorView>();
const accessExtension = (): Extension => (isCollabGuest() && !guestCanEdit() ? EditorState.readOnly.of(true) : []);

registerEditorExtension(() => [
  access.of(accessExtension()),
  ViewPlugin.define((view) => {
    guestViews.add(view);
    return { destroy: () => void guestViews.delete(view) };
  }),
]);

/** Reconfigure every open editor after this guest's permission changed. */
function applyAccess(): void {
  const ext = accessExtension();
  for (const view of guestViews) view.dispatch({ effects: access.reconfigure(ext) });
}

/** The window title: `<project> — <host name> (Code With Me)`. */
export function guestTitle(): string | undefined {
  const ws = session().workspace;
  return ws?.collab ? t("tools.codeWithMe.windowTitle", { project: ws.name, host: ws.collab.host }) : undefined;
}

let tracking = false;

/** Open the host's project when the backend says this window joined, close it when the session ends. */
export function trackGuestProject(): void {
  if (tracking) return;
  tracking = true;
  createRoot(() => {
    createEffect(
      on(
        () => (session().role === "guest" ? session().workspace : undefined),
        (ws) => {
          if (ws) adoptWorkspace(ws);
        },
      ),
    );
    createEffect(() => {
      document.title = guestTitle() ?? "Forge";
    });
    createEffect(
      on(
        () => (isCollabGuest() ? guestCanEdit() : undefined),
        (canEdit, before) => {
          applyAccess();
          // Told only when the host changes it during the session, not on joining.
          if (canEdit === undefined || before === undefined || canEdit === before) return;
          showBalloon(
            t("tools.codeWithMe.title"),
            canEdit ? t("tools.codeWithMe.hostLetsYouEdit") : t("tools.codeWithMe.filesReadOnlyNow"),
            5000,
          );
        },
      ),
    );
  });
  void collabApi.onEnded(({ reason }) => {
    if (!isCollabGuest()) return;
    closeWorkspace();
    showBalloon(t("tools.codeWithMe.title"), reason, 6000);
  });
}
