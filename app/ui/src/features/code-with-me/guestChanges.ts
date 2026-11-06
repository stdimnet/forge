/**
 * Lines guests changed, marked with a dot in the guest's color in the host's gutter until the
 * next save (the plan's guard against unnoticed changes: Files Edit lets a guest change code
 * the host runs later). Hovering a marker names the guest.
 *
 * The host's backend says whose edit an update is (`collab://doc` `from`); `docs.ts` applies
 * it inside {@link withAuthor}, and `y-codemirror.next` dispatches the editor transaction
 * synchronously within that call, so a transaction extender can tag it. Changes from disk and
 * the host's own typing carry no author. Positions follow every later edit; a save (or a
 * reload from disk) clears the markers.
 */
import { EditorState, RangeSet, StateEffect, StateField, type Extension, type Text } from "@codemirror/state";
import { EditorView, gutter, GutterMarker } from "@codemirror/view";
import { t } from "@/i18n";

export interface Author {
  /** The participant's color index (1-8, `--collab-<n>`). */
  color: number;
  name: string;
}

let current: Author | null = null;

/** Run `fn` (applying a remote update) with edits attributed to `author`. */
export function withAuthor<T>(author: Author | null, fn: () => T): T {
  const prev = current;
  current = author;
  try {
    return fn();
  } finally {
    current = prev;
  }
}

class GuestLine extends GutterMarker {
  constructor(
    readonly color: number,
    readonly name: string,
  ) {
    super();
  }
  eq(other: GuestLine): boolean {
    return other.color === this.color && other.name === this.name;
  }
  toDOM(): Node {
    const el = document.createElement("div");
    el.className = `cm-cwm-guest-line cm-cwm-guest-${this.color}`;
    el.title = t("tools.codeWithMe.changedBy", { name: this.name });
    el.setAttribute("aria-label", t("tools.codeWithMe.changedBy", { name: this.name }));
    return el;
  }
}

interface Marked {
  from: number;
  to: number;
  author: Author;
}

const markLines = StateEffect.define<Marked[]>();
/** Forget all markers (the document was saved). */
export const clearGuestLines = StateEffect.define<null>();

/** One marker per line start: mapped markers snap to their line, newer ones win. */
function rebuild(doc: Text, old: RangeSet<GuestLine>, mapPos: ((pos: number) => number) | null, marks: Marked[]): RangeSet<GuestLine> {
  const lines = new Map<number, GuestLine>();
  for (let it = old.iter(); it.value; it.next()) {
    const pos = mapPos ? mapPos(it.from) : it.from;
    lines.set(doc.lineAt(Math.min(pos, doc.length)).from, it.value);
  }
  for (const m of marks) {
    const marker = new GuestLine(m.author.color, m.author.name);
    const last = doc.lineAt(Math.min(m.to, doc.length)).number;
    for (let n = doc.lineAt(Math.min(m.from, doc.length)).number; n <= last; n++) lines.set(doc.line(n).from, marker);
  }
  return RangeSet.of(
    [...lines].sort(([a], [b]) => a - b).map(([pos, marker]) => marker.range(pos)),
    true,
  );
}

export const guestLines = StateField.define<RangeSet<GuestLine>>({
  create: () => RangeSet.empty,
  update(set, tr) {
    let cleared = false;
    const marks: Marked[] = [];
    for (const e of tr.effects) {
      if (e.is(clearGuestLines)) cleared = true;
      else if (e.is(markLines)) marks.push(...e.value);
    }
    if (cleared) set = RangeSet.empty;
    if (!tr.docChanged && !marks.length) return set;
    if (set.size === 0 && !marks.length) return set;
    return rebuild(tr.newDoc, set, tr.docChanged ? (pos) => tr.changes.mapPos(pos, 1) : null, marks);
  },
});

/** Line numbers (1-based) with a marker and who changed them (for tests). */
export function markedLines(state: EditorState): { line: number; color: number; name: string }[] {
  const out: { line: number; color: number; name: string }[] = [];
  for (let it = state.field(guestLines).iter(); it.value; it.next()) {
    out.push({ line: state.doc.lineAt(it.from).number, color: it.value.color, name: it.value.name });
  }
  return out;
}

/** Tag the transactions a guest's update causes with the lines it touched. */
const tagGuestEdits = EditorState.transactionExtender.of((tr) => {
  if (!current || !tr.docChanged) return null;
  const author = current;
  const marks: Marked[] = [];
  tr.changes.iterChangedRanges((_fromA, _toA, fromB, toB) => marks.push({ from: fromB, to: toB, author }));
  return { effects: markLines.of(marks) };
});

const COLORS = 8;

/** A dot, so it never reads as one of the VCS change bars next to it. */
const theme = EditorView.theme({
  ".cm-cwm-guest-gutter": { width: "8px" },
  ".cm-cwm-guest-gutter .cm-gutterElement": { padding: "0", display: "flex", alignItems: "center", justifyContent: "center" },
  ".cm-cwm-guest-line": { width: "6px", height: "6px", borderRadius: "50%" },
  ...Object.fromEntries(Array.from({ length: COLORS }, (_, i) => [`.cm-cwm-guest-${i + 1}`, { backgroundColor: `var(--collab-${i + 1})` }])),
});

/** The field, the tagging and the gutter for one shared document's editor (`binding.ts`). */
export function guestChanges(): Extension {
  return [
    guestLines,
    tagGuestEdits,
    gutter({
      class: "cm-cwm-guest-gutter",
      markers: (view) => view.state.field(guestLines),
    }),
    theme,
  ];
}
