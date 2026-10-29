/**
 * Presence in this window: where the other participants are (from `collab://presence` and
 * the session view), and where this participant is, sent to the session while a session
 * runs: the active editor's file, caret, selection and the lines in view, and whom this
 * window follows. Sending is throttled to 10 Hz and skips repeats.
 */
import { createEffect, createRoot, createSignal, on } from "solid-js";
import type { EditorView, ViewUpdate } from "@codemirror/view";
import { EditorView as View } from "@codemirror/view";
import { collabApi, type Caret, type Presence } from "@/ipc/collab";
import { registerEditorExtension } from "@/editor/extensions";
import { activeView } from "@/editor/views";
import { editor } from "@/state/editor";
import { presenceFile } from "./followState";
import { session } from "./store";

const [others, setOthers] = createSignal<Map<number, Presence>>(new Map());

/** Where the other participants are, by participant id. */
export const presence = others;
export const presenceOf = (id: number): Presence | undefined => others().get(id);

const listeners = new Set<(participant: number, p: Presence) => void>();

/** Called for every presence update of another participant. */
export function onPresence(cb: (participant: number, p: Presence) => void): () => void {
  listeners.add(cb);
  return () => listeners.delete(cb);
}

function store(participant: number, p: Presence): void {
  setOthers((m) => new Map(m).set(participant, p));
  listeners.forEach((l) => l(participant, p));
}

// ---- this participant ------------------------------------------------------------------

let following: () => number | null = () => null;

/** Whom this window follows, for the presence it sends (`follow.ts`). */
export function setFollowingSource(source: () => number | null): void {
  following = source;
}

const caretOf = (view: EditorView, pos: number): Caret => {
  const line = view.state.doc.lineAt(pos);
  return { line: line.number, column: pos - line.from + 1 };
};

/** This window's presence, from the active editor. */
export function localPresence(): Presence {
  const p: Presence = {};
  const file = presenceFile(editor.active);
  const leader = following();
  if (leader !== null) p.following = leader;
  if (!file) return p;
  p.file = file;
  const view = activeView();
  if (!view) return p;
  const sel = view.state.selection.main;
  p.caret = caretOf(view, sel.head);
  if (!sel.empty) p.anchor = caretOf(view, sel.anchor);
  const scroller = view.scrollDOM;
  if (scroller.clientHeight > 0) {
    const top = view.lineBlockAtHeight(Math.max(0, scroller.scrollTop - view.documentPadding.top));
    const bottom = view.lineBlockAtHeight(Math.max(0, scroller.scrollTop + scroller.clientHeight - view.documentPadding.top));
    p.viewport = { topLine: view.state.doc.lineAt(top.from).number, bottomLine: view.state.doc.lineAt(bottom.from).number };
  }
  return p;
}

const INTERVAL = 100;
let last = "";
let timer: ReturnType<typeof setTimeout> | null = null;
let lastSent = 0;

/** Send this window's presence soon (at most every 100 ms). */
export function schedulePresence(): void {
  if (session().role === "none" || timer) return;
  const wait = Math.max(0, lastSent + INTERVAL - Date.now());
  timer = setTimeout(() => {
    timer = null;
    lastSent = Date.now();
    sendNow();
  }, wait);
}

function sendNow(): void {
  if (session().role === "none") return;
  const p = localPresence();
  const json = JSON.stringify(p);
  if (json === last) return;
  last = json;
  void collabApi.presence(p).catch(() => undefined);
}

const onUpdate = (u: ViewUpdate) => {
  if ((u.selectionSet || u.viewportChanged || u.geometryChanged || u.docChanged) && u.view === activeView()) schedulePresence();
};

registerEditorExtension(() => [
  View.updateListener.of(onUpdate),
  View.domEventHandlers({
    scroll: (_e, view) => {
      if (view === activeView()) schedulePresence();
    },
  }),
]);

let tracking = false;

/** Follow the others' presence and send this window's while a session runs. */
export function trackPresence(): void {
  if (tracking) return;
  tracking = true;
  void collabApi.onPresence((p) => {
    if (session().role === "none" || p.participant === session().me) return;
    const { participant, ...rest } = p;
    store(participant, rest);
  });
  createRoot(() => {
    // The session view carries everyone's presence (after joining, after a reload).
    createEffect(() => {
      const s = session();
      if (s.role === "none") {
        if (others().size) setOthers(new Map());
        last = "";
        return;
      }
      const ids = new Set(s.participants.map((p) => p.id));
      const next = new Map([...others()].filter(([id]) => ids.has(id)));
      for (const { participant, ...rest } of s.presence ?? []) if (!next.has(participant)) next.set(participant, rest);
      if (next.size !== others().size || [...next.keys()].some((id) => !others().has(id))) setOthers(next);
    });
    createEffect(on(() => [session().role, editor.active] as const, schedulePresence));
  });
}
