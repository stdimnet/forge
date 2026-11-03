/**
 * Join Session…: the action `CodeWithMe.JoinSession`, and a clicked `forge://cwm/join?…` link,
 * which the backend checked and routed to this window (`companion://join-session`). Both open
 * the Join Session dialog; a link only fills it in, nothing connects before Join.
 */
import { showBalloon } from "@/keymap";
import { hideWaiting } from "./approval";
import { openJoinDialog } from "./JoinSession";
import { applySession, isGuest, isHost, setPopupOpen } from "./store";

export async function joinSession(link = ""): Promise<void> {
  setPopupOpen(false);
  if (isHost() || isGuest()) return void showBalloon("Code With Me", "A session is already running.");
  try {
    const s = await openJoinDialog(link);
    if (s) applySession(s);
  } finally {
    hideWaiting();
  }
}
