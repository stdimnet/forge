/**
 * The host's session actions: the session's permissions, one guest's permissions, Remove
 * Guest and Regenerate Link, with their menus (popup, participant rows).
 */
import { collabApi, levelOf, PRESETS, type Participant, type PermissionLevel, type Permissions } from "@/ipc/collab";
import { showBalloon } from "@/keymap";
import { askConfirm } from "@/features/contextMenu/dialogs";
import { item, separator, showContextMenu, submenu, type MenuEntry } from "@/features/contextMenu/state";
import { askCustomPermissions } from "./CustomPermissions";
import { applySession, isHost, refreshSession, session } from "./store";

const message = (e: unknown) => String((e as Error)?.message ?? e);

export const LEVEL_TITLES: Record<PermissionLevel, string> = {
  readOnly: "Read-only",
  editFiles: "Edit files",
  fullAccess: "Full access",
  custom: "Custom",
};

const PRESET_LEVELS = ["readOnly", "editFiles", "fullAccess"] as const;

/** The session's guest permissions (host). */
export const sessionPermissions = (): Permissions => session().permissions ?? PRESETS.readOnly;

/** Permissions for the whole session: new guests and every guest in it. */
export async function setSessionPermissions(p: Permissions): Promise<void> {
  try {
    applySession(await collabApi.setPermissions(p));
    showBalloon("Code With Me", `Guests now have ${LEVEL_TITLES[levelOf(p)]} permissions.`, 4000);
  } catch (e) {
    showBalloon("Cannot change the permissions", message(e), 6000);
  }
}

export async function setGuestPermissions(guest: Participant, p: Permissions): Promise<void> {
  try {
    await collabApi.setGuestPermissions(guest.id, p);
    await refreshSession();
  } catch (e) {
    showBalloon("Cannot change the permissions", message(e), 6000);
  }
}

/** Permissions… (Custom): the dialog for the whole session. */
export async function editSessionPermissions(): Promise<void> {
  if (!isHost()) return void showBalloon("Code With Me", "No session is running.");
  const p = await askCustomPermissions("Custom Permissions", sessionPermissions());
  if (p) await setSessionPermissions(p);
}

export async function removeGuest(guest: Participant): Promise<void> {
  const ok = await askConfirm({
    title: "Remove Guest",
    message: `Remove ${guest.name} from the session? They can ask to join again with the link unless you regenerate it.`,
    confirm: "Remove",
  });
  if (!ok) return;
  try {
    await collabApi.removeGuest(guest.id);
    await refreshSession();
  } catch (e) {
    showBalloon("Cannot remove the guest", message(e), 6000);
  }
}

async function copy(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    return false;
  }
}

/** Regenerate Link: the old link stops working; the new one is copied. */
export async function regenerateLink(): Promise<void> {
  if (!isHost()) return void showBalloon("Code With Me", "No session is running.");
  const ok = await askConfirm({
    title: "Regenerate Link",
    message: "The current session link stops working. Guests in the session stay. Create a new link?",
    confirm: "Regenerate",
  });
  if (!ok) return;
  try {
    const link = await collabApi.regenerateLink();
    await refreshSession();
    showBalloon(
      "Code With Me",
      (await copy(link)) ? "A new session link is copied to the clipboard. The old link no longer works." : link,
      6000,
    );
  } catch (e) {
    showBalloon("Cannot regenerate the link", message(e), 6000);
  }
}

/** The four levels, checked at `current`; Custom… asks for the values. */
export function permissionEntries(current: Permissions, title: string, apply: (p: Permissions) => void): MenuEntry[] {
  const level = levelOf(current);
  return [
    ...PRESET_LEVELS.map((l) => item(LEVEL_TITLES[l], () => apply(PRESETS[l]), undefined, { checked: level === l })),
    item(
      "Custom…",
      () =>
        void askCustomPermissions(title, current).then((p) => {
          if (p) apply(p);
        }),
      undefined,
      { checked: level === "custom" },
    ),
  ];
}

/** The session's Permissions menu (popup). */
export function showSessionPermissionsMenu(x: number, y: number): void {
  showContextMenu(
    permissionEntries(sessionPermissions(), "Custom Permissions", (p) => void setSessionPermissions(p)),
    x,
    y,
  );
}

/** The host's entries of a guest's menu in the participant list. */
export function guestEntries(guest: Participant): MenuEntry[] {
  return [
    submenu(
      "Permissions",
      permissionEntries(guest.permissions, `Permissions of ${guest.name}`, (p) => void setGuestPermissions(guest, p)),
    ),
    separator,
    item("Remove Guest", () => void removeGuest(guest)),
  ];
}

/** A guest's context menu in the participant list (host). */
export function showGuestMenu(guest: Participant, x: number, y: number): void {
  showContextMenu(guestEntries(guest), x, y);
}
