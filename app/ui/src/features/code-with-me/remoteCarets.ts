/**
 * The other participants' carets beyond what `y-codemirror.next` draws (PHPStorm Code With
 * Me):
 *
 * - the name label above a remote caret shows while the participant moves or types and fades
 *   out {@link LABEL_MS} after the caret stopped (hovering the caret shows it again);
 * - every remote caret has a mark in its participant's color on the error stripe (the right
 *   edge of the editor, over the scrollbar), so you see where the others are in the file;
 *   clicking it moves your caret there.
 *
 * Both follow the shared document's awareness; nothing a peer sends ends up in CSS (colors are
 * the theme's `--collab-<n>` tokens, picked by the color index the host stamped).
 */
import { StateEffect, StateField, type Extension } from "@codemirror/state";
import { EditorView, ViewPlugin, type PluginValue } from "@codemirror/view";
import * as Y from "yjs";
import type { Awareness } from "y-protocols/awareness";
import { stripeMarks, type StripeMark } from "@/editor/errorStripe";

/** How long a name label stays after the caret stopped moving. */
export const LABEL_MS = 2000;

const COLORS = 8;

const colorOf = (state: unknown): number => {
  const n = Math.floor(Number((state as { user?: { colorIndex?: unknown } })?.user?.colorIndex));
  return Number.isFinite(n) ? Math.min(COLORS, Math.max(1, n)) : 1;
};

const nameOf = (state: unknown): string => String((state as { user?: { name?: unknown } })?.user?.name ?? "Guest").slice(0, 64);

/** Where the remote carets are: one stripe mark per participant with a caret in this file. */
export function caretMarks(awareness: Awareness, ytext: Y.Text): StripeMark[] {
  const doc = ytext.doc;
  if (!doc) return [];
  const marks: StripeMark[] = [];
  awareness.getStates().forEach((state, client) => {
    if (client === doc.clientID) return;
    const head = (state as { cursor?: { head?: unknown } })?.cursor?.head;
    if (!head) return;
    const abs = Y.createAbsolutePositionFromRelativePosition(Y.createRelativePositionFromJSON(head), doc);
    if (!abs || abs.type !== ytext) return;
    marks.push({ pos: abs.index, cls: `cm-stripe-collab-${colorOf(state)}`, title: nameOf(state) });
  });
  return marks.sort((a, b) => a.pos - b.pos);
}

const setMarks = StateEffect.define<readonly StripeMark[]>();

/** The remote carets' stripe marks; positions follow local edits until the next awareness update. */
const caretField = StateField.define<readonly StripeMark[]>({
  create: () => [],
  update(marks, tr) {
    for (const e of tr.effects) if (e.is(setMarks)) return e.value;
    if (!tr.docChanged || !marks.length) return marks;
    return marks.map((m) => ({ ...m, pos: tr.changes.mapPos(m.pos) }));
  },
  provide: (f) => stripeMarks.from(f),
});

/** Color indexes whose label shows: the caret moved less than {@link LABEL_MS} ago. */
export function activeLabels(moved: ReadonlyMap<number, number>, now: number): number[] {
  return [...moved].filter(([, at]) => now - at < LABEL_MS).map(([color]) => color);
}

/** The labels and stripe marks of one shared document's editor. */
function caretActivity(awareness: Awareness, ytext: Y.Text) {
  return ViewPlugin.define((view): PluginValue & { labels: number[] } => {
    /** Last movement per color index. */
    const moved = new Map<number, number>();
    let timer: ReturnType<typeof setTimeout> | undefined;
    let destroyed = false;
    const plugin = {
      labels: [] as number[],
      refresh() {
        if (destroyed) return;
        const now = Date.now();
        plugin.labels = activeLabels(moved, now);
        clearTimeout(timer);
        const next = Math.min(...[...moved.values()].map((at) => at + LABEL_MS).filter((t) => t > now));
        if (Number.isFinite(next)) timer = setTimeout(() => plugin.refresh(), next - now + 10);
        view.dispatch({ effects: setMarks.of(caretMarks(awareness, ytext)) });
      },
      destroy() {
        destroyed = true;
        clearTimeout(timer);
        awareness.off("change", onChange);
      },
    };
    const onChange = ({ added, updated, removed }: { added: number[]; updated: number[]; removed: number[] }) => {
      const local = ytext.doc?.clientID;
      const remote = [...added, ...updated].filter((c) => c !== local);
      if (!remote.length && !removed.some((c) => c !== local)) return;
      const now = Date.now();
      for (const client of remote) moved.set(colorOf(awareness.getStates().get(client)), now);
      // Local awareness changes happen inside an editor update (y-codemirror sets the cursor there).
      queueMicrotask(() => plugin.refresh());
    };
    awareness.on("change", onChange);
    queueMicrotask(() => {
      // Everyone already in the file shows their name once when the editor binds.
      const now = Date.now();
      awareness.getStates().forEach((state, client) => {
        if (client !== ytext.doc?.clientID) moved.set(colorOf(state), now);
      });
      plugin.refresh();
    });
    return plugin;
  });
}

/** Labels show only for the colors in `cwm-label-<n>`, and on hover; they fade in and out. */
const labelTheme = EditorView.theme({
  ".cm-ySelectionInfo": { opacity: 0, transition: "opacity .3s ease-in-out", pointerEvents: "none" },
  ".cm-ySelectionCaret:hover > .cm-ySelectionInfo": { opacity: 1 },
  ...Object.fromEntries(
    Array.from({ length: COLORS }, (_, i) => [`&.cwm-label-${i + 1} .cm-ySelectionCaret[style*="--collab-${i + 1})"] > .cm-ySelectionInfo`, { opacity: 1 }]),
  ),
  ...Object.fromEntries(Array.from({ length: COLORS }, (_, i) => [`.cm-stripe-collab-${i + 1}`, { backgroundColor: `var(--collab-${i + 1})` }])),
});

/** What a shared document's editor adds for the remote carets (bound in `binding.ts`). */
export function remoteCarets(awareness: Awareness, ytext: Y.Text): Extension {
  const activity = caretActivity(awareness, ytext);
  return [
    caretField,
    activity,
    labelTheme,
    EditorView.editorAttributes.of((view) => {
      const labels = view.plugin(activity)?.labels ?? [];
      return labels.length ? { class: labels.map((n) => `cwm-label-${n}`).join(" ") } : null;
    }),
  ];
}
