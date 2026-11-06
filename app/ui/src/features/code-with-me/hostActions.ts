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
import { writeClipboardText } from "@/features/clipboard/system";
import { t } from "@/i18n";

const message = (e: unknown) => String((e as Error)?.message ?? e);

/** The permission levels' names, in the IDE language. */
export const LEVEL_TITLES: Readonly<Record<PermissionLevel, string>> = {
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

const PRESET_LEVELS = ["readOnly", "editFiles", "fullAccess"] as const;

/** The session's guest permissions (host). */
export const sessionPermissions = (): Permissions => session().permissions ?? PRESETS.readOnly;

/** Permissions for the whole session: new guests and every guest in it. */
export async function setSessionPermissions(p: Permissions): Promise<void> {
  try {
    applySession(await collabApi.setPermissions(p));
    showBalloon(t("tools.codeWithMe.title"), t("tools.codeWithMe.guestsNowHave", { level: LEVEL_TITLES[levelOf(p)] }), 4000);
  } catch (e) {
    showBalloon(t("tools.codeWithMe.cannotChangePermissions"), message(e), 6000);
  }
}

export async function setGuestPermissions(guest: Participant, p: Permissions): Promise<void> {
  try {
    await collabApi.setGuestPermissions(guest.id, p);
    await refreshSession();
  } catch (e) {
    showBalloon(t("tools.codeWithMe.cannotChangePermissions"), message(e), 6000);
  }
}

/** Permissions… (Custom): the dialog for the whole session. */
export async function editSessionPermissions(): Promise<void> {
  if (!isHost()) return void showBalloon(t("tools.codeWithMe.title"), t("tools.codeWithMe.noSessionRunning"));
  const p = await askCustomPermissions(t("tools.codeWithMe.customPermissions"), sessionPermissions());
  if (p) await setSessionPermissions(p);
}

export async function removeGuest(guest: Participant): Promise<void> {
  const ok = await askConfirm({
    title: t("tools.codeWithMe.removeGuest"),
    message: t("tools.codeWithMe.removeGuestQuestion", { name: guest.name }),
    confirm: t("common.remove"),
  });
  if (!ok) return;
  try {
    await collabApi.removeGuest(guest.id);
    await refreshSession();
  } catch (e) {
    showBalloon(t("tools.codeWithMe.cannotRemoveGuest"), message(e), 6000);
  }
}

async function copy(text: string): Promise<boolean> {
  try {
    await writeClipboardText(text);
    return true;
  } catch {
    return false;
  }
}

/** Regenerate Link: the old link stops working; the new one is copied. */
export async function regenerateLink(): Promise<void> {
  if (!isHost()) return void showBalloon(t("tools.codeWithMe.title"), t("tools.codeWithMe.noSessionRunning"));
  const ok = await askConfirm({
    title: t("tools.codeWithMe.regenerateLink"),
    message: t("tools.codeWithMe.regenerateLinkQuestion"),
    confirm: t("tools.codeWithMe.regenerate"),
  });
  if (!ok) return;
  try {
    const link = await collabApi.regenerateLink();
    await refreshSession();
    showBalloon(t("tools.codeWithMe.title"), (await copy(link)) ? t("tools.codeWithMe.newLinkCopied") : link, 6000);
  } catch (e) {
    showBalloon(t("tools.codeWithMe.cannotRegenerateLink"), message(e), 6000);
  }
}

/** The four levels, checked at `current`; Custom… asks for the values. */
export function permissionEntries(current: Permissions, title: string, apply: (p: Permissions) => void): MenuEntry[] {
  const level = levelOf(current);
  return [
    ...PRESET_LEVELS.map((l) => item(LEVEL_TITLES[l], () => apply(PRESETS[l]), undefined, { checked: level === l })),
    item(
      t("tools.codeWithMe.customEllipsis"),
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
    permissionEntries(sessionPermissions(), t("tools.codeWithMe.customPermissions"), (p) => void setSessionPermissions(p)),
    x,
    y,
  );
}

/** The host's entries of a guest's menu in the participant list. */
export function guestEntries(guest: Participant): MenuEntry[] {
  return [
    submenu(
      t("tools.codeWithMe.permissions"),
      permissionEntries(guest.permissions, t("tools.codeWithMe.permissionsOf", { name: guest.name }), (p) => void setGuestPermissions(guest, p)),
    ),
    separator,
    item(t("tools.codeWithMe.removeGuest"), () => void removeGuest(guest)),
  ];
}

/** A guest's context menu in the participant list (host). */
export function showGuestMenu(guest: Participant, x: number, y: number): void {
  showContextMenu(guestEntries(guest), x, y);
}
