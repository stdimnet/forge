/**
 * Host approval: a guest that asks to join shows up as a balloon that stays until the host
 * answers. The balloon names the guest, the permissions it would get and the verification
 * code, which the guest sees as well (the host compares them over a call or chat). The
 * guest's side is the waiting balloon with the same code and Cancel.
 */
import { showBalloon, dismissBalloon } from "@/keymap";
import { collabApi, type JoinRequest, type PermissionLevel } from "@/ipc/collab";
import { LEVEL_NAMES } from "./guest";
import { joinDialogOpen, showJoinWaiting } from "./JoinSession";
import { t } from "@/i18n";

const message = (e: unknown) => String((e as Error)?.message ?? e);

const open = new Map<number, number>();

/** What the host is asked, per level: the code to compare and what the guest may do. */
const joinRequestText = (level: PermissionLevel, name: string, code: string): string => {
  const args = { name, code, level: LEVEL_NAMES[level] };
  switch (level) {
    case "readOnly":
      return t("tools.codeWithMe.joinRequest.readOnly", args);
    case "editFiles":
      return t("tools.codeWithMe.joinRequest.editFiles", args);
    case "fullAccess":
      return t("tools.codeWithMe.joinRequest.fullAccess", args);
    default:
      return t("tools.codeWithMe.joinRequest.custom", args);
  }
};

function close(id: number): void {
  const balloon = open.get(id);
  open.delete(id);
  if (balloon !== undefined) dismissBalloon(balloon);
}

async function answer(id: number, accept: boolean): Promise<void> {
  close(id);
  try {
    await collabApi.answerJoin(id, accept);
  } catch (e) {
    showBalloon(t("tools.codeWithMe.title"), message(e), 5000);
  }
}

/** Ask the host about `request`. */
export function showJoinRequest(request: JoinRequest): void {
  const { id, name, code, level } = request;
  const balloon = showBalloon(
    t("tools.codeWithMe.wantsToJoin", { name }),
    joinRequestText(level, name, code),
    0,
    [
      { label: t("tools.codeWithMe.accept"), run: () => void answer(id, true) },
      { label: t("tools.codeWithMe.decline"), run: () => void answer(id, false) },
    ],
    // Never hidden by a notification setting: the guest waits for this answer.
    { log: false },
  );
  open.set(id, balloon);
}

let waiting: number | undefined;

/** The guest waits for the host: the code to compare, and Cancel. */
export function showWaiting(host: string, code: string): void {
  hideWaiting();
  waiting = showBalloon(
    t("tools.codeWithMe.waitingForHost", { host }),
    t("tools.codeWithMe.verificationCodeDetail", { host, code }),
    0,
    [{ label: t("common.cancel"), run: () => void collabApi.cancelJoin().catch(() => undefined) }],
    { log: false },
  );
}

export function hideWaiting(): void {
  if (waiting !== undefined) dismissBalloon(waiting);
  waiting = undefined;
}

let tracking = false;

/** Follow the join requests of this window's session. */
export function trackApprovals(): void {
  if (tracking) return;
  tracking = true;
  void collabApi.onJoinRequest(showJoinRequest);
  void collabApi.onJoinClosed(({ id }) => close(id));
  // The Join Session dialog shows the code itself; a join from a link elsewhere gets the balloon.
  void collabApi.onWaiting((w) => (joinDialogOpen() ? showJoinWaiting(w.host, w.code) : showWaiting(w.host, w.code)));
}
