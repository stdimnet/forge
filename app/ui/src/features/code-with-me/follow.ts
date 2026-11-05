/**
 * Follow, Jump To and Make Everyone Follow You (PHPStorm Code With Me).
 *
 * - Follow: open the leader's file and keep the leader's viewport (top line) in view on each
 *   presence update. Typing, clicking into the editor or opening another file stops it, as
 *   does Stop in the banner above the editor.
 * - Jump To: open the participant's file at its caret once, without following.
 * - Make Everyone Follow You (host): every guest follows the host; guests can still stop.
 * - Guests follow the host when they join (`codeWithMe.followHostOnJoin`).
 *
 * The state machine is `followState.ts`; this module drives the editor with it.
 */
import { batch, createEffect, createRoot, createSignal, on } from "solid-js";
import { showPanel, type Panel, EditorView } from "@codemirror/view";
import type { Participant } from "@/ipc/collab";
import { collabApi } from "@/ipc/collab";
import { registerEditorExtension } from "@/editor/extensions";
import { openFileAt, visibleView } from "@/editor/views";
import { editor, openFile } from "@/state/editor";
import { getSetting } from "@/state/settings";
import { showBalloon } from "@/keymap";
import { h } from "@/features/lsp/util";
import { t } from "@/i18n";
import { bannerText, followTarget, nextFollow, NOT_FOLLOWING, type FollowEvent, type FollowState } from "./followState";
import { onPresence, presenceOf, schedulePresence, setFollowingSource } from "./presence";
import { applySession, session } from "./store";

const [state, setState] = createSignal<FollowState>(NOT_FOLLOWING);
export const followState = state;
export const leader = () => state().leader;

setFollowingSource(() => state().leader);

/** The tab following leads to; switching to another one stops following. */
let expectedTab: string | null = null;
/** Navigation by Forge itself (following, opening the host's files on joining): no stopping. */
let quiet = 0;

/** Run `fn` without it counting as the user's own navigation. */
export async function withoutStopping<T>(fn: () => Promise<T>): Promise<T> {
  quiet++;
  try {
    return await fn();
  } finally {
    quiet--;
  }
}

function dispatch(e: FollowEvent): void {
  const before = state();
  const after = nextFollow(before, e);
  if (after === before) return;
  setState(after);
  if (after.leader === null) expectedTab = null;
  schedulePresence();
  if (after.leader !== null && after.leader !== before.leader) void goToLeader();
}

const hostOf = (): Participant | undefined => session().participants.find((p) => p.role === "host");

/** Follow a participant. */
export function follow(id: number): void {
  dispatch({ type: "follow", leader: id, me: session().me });
}

export function stopFollowing(): void {
  dispatch({ type: "stop" });
}

/** The user acted in the editor (typed, clicked, opened another file). */
function localAction(): void {
  if (state().leader !== null && quiet === 0) dispatch({ type: "local" });
}

/** Open the leader's file and scroll to its viewport. */
async function goToLeader(): Promise<void> {
  const id = state().leader;
  if (id === null) return;
  const target = followTarget(presenceOf(id), session().role);
  if (!target) return;
  expectedTab = target.tab;
  await withoutStopping(async () => {
    if (editor.active !== target.tab) await openFile(target.tab);
    const view = await visibleView(target.tab);
    if (!view || state().leader !== id) return;
    const doc = view.state.doc;
    if (target.topLine) {
      const line = doc.line(Math.min(Math.max(1, target.topLine), doc.lines));
      view.dispatch({ effects: EditorView.scrollIntoView(line.from, { y: "start" }) });
    } else if (target.caret) {
      const line = doc.line(Math.min(Math.max(1, target.caret.line), doc.lines));
      view.dispatch({ effects: EditorView.scrollIntoView(line.from, { y: "center" }) });
    }
  }).catch((e) => console.warn("cannot follow", e));
}

/** Jump To: open the participant's file at its caret, once. */
export async function jumpTo(p: Participant): Promise<void> {
  const target = followTarget(presenceOf(p.id), session().role);
  if (!target) return void showBalloon(t("tools.codeWithMe.title"), t("tools.codeWithMe.noVisibleFile", { name: p.name }), 4000);
  if (state().leader !== null) stopFollowing();
  await openFileAt(target.tab, target.caret?.line, target.caret?.column, { flash: false });
}

/** Make Everyone Follow You on or off (host). */
export async function setFollowEveryone(on: boolean): Promise<void> {
  try {
    applySession(await collabApi.followEveryone(on));
    showBalloon(
      t("tools.codeWithMe.title"),
      on ? t("tools.codeWithMe.everyoneFollowsYou") : t("tools.codeWithMe.noLongerFollowYou"),
      4000,
    );
  } catch (e) {
    showBalloon(t("tools.codeWithMe.title"), String((e as Error)?.message ?? e), 6000);
  }
}

// ---- the editor --------------------------------------------------------------------------

const STOPPING_EVENTS = ["input", "delete", "move", "select", "undo", "redo"];

/** The banner above the editor while following: "Following Alex" with Stop. */
function followBanner(): Panel {
  const dom = h("div", "cwm-follow-banner");
  dom.setAttribute("role", "status");
  const dot = h("span", "cwm-follow-dot");
  const text = h("span", "cwm-follow-text");
  const stop = h("button", "link-btn cwm-follow-stop", t("tools.codeWithMe.stop"));
  stop.onclick = () => stopFollowing();
  dom.append(dot, text, stop);
  let dispose = () => {};
  createRoot((d) => {
    dispose = d;
    createEffect(() => {
      const label = bannerText(state(), session().participants);
      dom.hidden = !label;
      text.textContent = label ?? "";
      const color = session().participants.find((p) => p.id === state().leader)?.color ?? 1;
      dom.style.setProperty("--cwm-follow-color", `var(--collab-${Math.min(8, Math.max(1, color))})`);
    });
  });
  return { dom, top: true, destroy: () => dispose() };
}

registerEditorExtension(() => [
  showPanel.of(followBanner),
  EditorView.updateListener.of((u) => {
    if (state().leader === null) return;
    if (u.transactions.some((tr) => STOPPING_EVENTS.some((ev) => tr.isUserEvent(ev)))) localAction();
  }),
  EditorView.domEventHandlers({
    mousedown: () => {
      localAction();
      return false;
    },
  }),
]);

// ---- the session -------------------------------------------------------------------------

/** Guests start by following the host (after `session.ts` opened the shared files). */
export function followHostOnJoin(): void {
  const host = hostOf();
  if (!host || session().role !== "guest") return;
  // Asked to follow while the shared files were opening: show the host's file now.
  if (state().leader === host.id) void goToLeader();
  else if (getSetting<boolean>("codeWithMe.followHostOnJoin", true)) follow(host.id);
}

let tracking = false;

export function trackFollow(): void {
  if (tracking) return;
  tracking = true;
  onPresence((id) => {
    if (id === state().leader) void goToLeader();
  });
  void collabApi.onFollowHost(({ on }) => {
    const host = hostOf();
    if (session().role !== "guest" || !host) return;
    dispatch({ type: "forced", on, host: host.id });
    if (on) showBalloon(t("tools.codeWithMe.title"), t("tools.codeWithMe.askedEveryoneToFollowBalloon", { name: host.name }), 4000);
  });
  createRoot(() => {
    createEffect(
      on(
        () => session().role,
        (role, prev) => {
          if (role === "none") dispatch({ type: "ended" });
          // A guest that joins while the host asks everyone to follow.
          const host = hostOf();
          if (role === "guest" && prev !== "guest" && session().followEveryone && host) dispatch({ type: "forced", on: true, host: host.id });
        },
      ),
    );
    createEffect(() => {
      const ids = session().participants.map((p) => p.id);
      batch(() => dispatch({ type: "participants", ids }));
    });
    // Opening another file stops following.
    createEffect(
      on(
        () => editor.active,
        (active) => {
          if (state().leader === null || quiet > 0 || expectedTab === null) return;
          if (active && active !== expectedTab) localAction();
        },
        { defer: true },
      ),
    );
  });
}
