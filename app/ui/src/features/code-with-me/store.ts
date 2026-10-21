/**
 * Code With Me session state for the UI: the session as the backend reports it and the
 * toolbar popup.
 */
import { createSignal } from "solid-js";
import { collabApi, type SessionView } from "@/ipc/collab";

const idle: SessionView = { role: "none", canHost: false, participants: [], files: [] };

const [session, setSession] = createSignal<SessionView>(idle);
export { session };

const [popupOpen, setPopupOpen] = createSignal(false);
export { popupOpen, setPopupOpen };

export const inSession = () => session().role !== "none";
export const isHost = () => session().role === "host";
export const isGuest = () => session().role === "guest";

let tracking = false;

/** Load the session and follow `collab://status`. */
export function trackSession(): void {
  if (tracking) return;
  tracking = true;
  void collabApi.onStatus((s) => setSession(s));
  void refreshSession();
}

export async function refreshSession(): Promise<void> {
  try {
    setSession(await collabApi.status());
  } catch (e) {
    console.warn("Code With Me status unavailable", e);
  }
}

/** Set the session directly (after a command answered with the new state). */
export function applySession(s: SessionView): void {
  setSession(s);
}
