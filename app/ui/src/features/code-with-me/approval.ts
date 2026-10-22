/**
 * Host approval: a guest that asks to join shows up as a balloon that stays until the host
 * answers. The balloon names the guest, the permissions it would get and the verification
 * code, which the guest sees as well (the host compares them over a call or chat). The
 * guest's side is the waiting balloon with the same code and Cancel.
 */
import { showBalloon, dismissBalloon } from "@/keymap";
import { collabApi, type JoinRequest, type PermissionLevel } from "@/ipc/collab";
import { LEVEL_NAMES } from "./guest";

const message = (e: unknown) => String((e as Error)?.message ?? e);

const open = new Map<number, number>();

const WHAT: Record<PermissionLevel, string> = {
  readOnly: "view files",
  editFiles: "edit files",
  fullAccess: "edit files, use the terminal and run code on your machine",
  custom: "use the permissions you set",
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
    showBalloon("Code With Me", message(e), 5000);
  }
}

/** Ask the host about `request`. */
export function showJoinRequest(request: JoinRequest): void {
  const { id, name, code, level } = request;
  const balloon = showBalloon(
    `${name} wants to join the session`,
    `Make sure the code is ${code}. With ${LEVEL_NAMES[level]} permission ${name} can ${WHAT[level]}.`,
    0,
    [
      { label: "Accept", run: () => void answer(id, true) },
      { label: "Decline", run: () => void answer(id, false) },
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
    `Waiting for ${host} to accept…`,
    `Verification code: ${code}. ${host} sees the same code.`,
    0,
    [{ label: "Cancel", run: () => void collabApi.cancelJoin().catch(() => undefined) }],
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
  void collabApi.onWaiting((w) => showWaiting(w.host, w.code));
}
