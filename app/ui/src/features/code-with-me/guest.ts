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

/** This guest's permissions (`undefined` unless this window is a guest). */
export const guestPermissions = (): Permissions | undefined => (session().role === "guest" ? session().permissions : undefined);

/** Guests can create, rename, delete and edit files only with Files: Edit. */
export const guestCanEdit = (): boolean => !isCollabGuest() || guestPermissions()?.files === "edit";

export const guestLevel = (): PermissionLevel | undefined => {
  const p = guestPermissions();
  return p ? levelOf(p) : undefined;
};

export const LEVEL_NAMES: Record<PermissionLevel, string> = {
  readOnly: "Read-only",
  editFiles: "Edit files",
  fullAccess: "Full access",
  custom: "Custom",
};

/**
 * Tool windows a guest does not get. AI Chat, Database, Services and Pull Requests would use
 * the host's secrets and accounts or the guest's own, and the Debug window controls the
 * host's debugger. Terminal and Run come with shared terminals and runs; the Commit window
 * writes to the host's repository, so it needs Full access to version control.
 */
export function hiddenForGuest(id: string): boolean {
  if (!isCollabGuest()) return false;
  if (["ai-chat", "database", "services", "debug", "pull-requests", "terminal", "run"].includes(id)) return true;
  if (id === "commit") return guestPermissions()?.vcs !== "full";
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
  return ws?.collab ? `${ws.name} — ${ws.collab.host} (Code With Me)` : undefined;
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
          if (canEdit === undefined || before === undefined) return;
          showBalloon(
            "Code With Me",
            canEdit ? "The host lets you edit files now." : "The host changed your permission: files are read-only now.",
            5000,
          );
        },
      ),
    );
  });
  void collabApi.onEnded(({ reason }) => {
    if (!isCollabGuest()) return;
    closeWorkspace();
    showBalloon("Code With Me", reason, 6000);
  });
}
