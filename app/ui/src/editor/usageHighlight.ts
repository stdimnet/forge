/**
 * Highlighted usages in an editor: the automatic highlighting of the element at the caret
 * and the sticky one of Highlight Usages in File (Ctrl+Shift+F7), which stays until
 * Escape and lets F3 / Shift+F3 jump between the usages. Reads and writes have their own
 * colors (`--usage-read-bg`, `--usage-write-bg`), also on the error stripe. The usages
 * come from Rust (`features/codeintel/highlightUsages.ts`); this module only holds them.
 */
import { EditorSelection, Prec, StateEffect, StateField, type EditorState, type Extension } from "@codemirror/state";
import { Decoration, EditorView, keymap, type DecorationSet } from "@codemirror/view";
import { stripeMarks, type StripeMark } from "./errorStripe";

export type UsageMarkKind = "read" | "write" | "text";

export interface UsageMark {
  from: number;
  to: number;
  kind: UsageMarkKind;
}

export interface UsageHighlights {
  marks: UsageMark[];
  /** Highlight Usages in File: kept until Escape (edits move the marks along). */
  sticky: boolean;
  /** The status bar text ("3 usages of 'total' found"). */
  summary?: string;
}

export const setUsageHighlights = StateEffect.define<UsageHighlights | null>();

const markClass: Record<UsageMarkKind, string> = {
  read: "cm-usage-read",
  write: "cm-usage-write",
  text: "cm-usage-read",
};

function decorations(h: UsageHighlights | null): DecorationSet {
  if (!h?.marks.length) return Decoration.none;
  return Decoration.set(
    h.marks.filter((m) => m.to > m.from).map((m) => Decoration.mark({ class: markClass[m.kind] }).range(m.from, m.to)),
    true,
  );
}

function stripe(h: UsageHighlights | null): StripeMark[] {
  return (h?.marks ?? []).map((m) => ({
    pos: m.from,
    cls: m.kind === "write" ? "cm-stripe-usage-write" : "cm-stripe-usage-read",
    title: h?.summary,
  }));
}

const NO_MARKS: StripeMark[] = [];

export const usageHighlightField = StateField.define<UsageHighlights | null>({
  create: () => null,
  update(value, tr) {
    for (const e of tr.effects) if (e.is(setUsageHighlights)) return e.value;
    if (!value || !tr.docChanged) return value;
    // The automatic highlight is outdated by any edit; the sticky one follows it.
    if (!value.sticky) return null;
    const marks = value.marks
      .map((m) => ({ ...m, from: tr.changes.mapPos(m.from, 1), to: tr.changes.mapPos(m.to, -1) }))
      .filter((m) => m.to > m.from);
    return { ...value, marks };
  },
  provide: (f) => [
    EditorView.decorations.from(f, decorations),
    stripeMarks.compute([f], (s) => {
      const h = s.field(f);
      return h?.marks.length ? stripe(h) : NO_MARKS;
    }),
  ],
});

export const usageHighlights = (state: EditorState): UsageHighlights | null => state.field(usageHighlightField, false) ?? null;

/** Whether Highlight Usages in File is showing its usages. */
export const hasStickyUsages = (state: EditorState): boolean => usageHighlights(state)?.sticky ?? false;

/** Remove the highlighted usages. */
export function clearUsageHighlights(view: EditorView): boolean {
  if (!usageHighlights(view.state)) return false;
  view.dispatch({ effects: setUsageHighlights.of(null) });
  return true;
}

const listeners = new Set<(view: EditorView) => void>();

/** Called when Escape removes the sticky highlighting (the status bar clears its text). */
export function onUsageHighlightsCleared(fn: (view: EditorView) => void): () => void {
  listeners.add(fn);
  return () => listeners.delete(fn);
}

/**
 * F3 / Shift+F3 while Highlight Usages in File is shown: select the next (previous) usage,
 * wrapping around. `false` when nothing is highlighted, so Find Next runs instead.
 */
export function gotoHighlightedUsage(view: EditorView, delta: 1 | -1): boolean {
  const h = usageHighlights(view.state);
  if (!h?.sticky || !h.marks.length) return false;
  const marks = [...h.marks].sort((a, b) => a.from - b.from);
  const at = view.state.selection.main.from;
  const after = marks.filter((m) => m.from > at);
  const before = marks.filter((m) => m.from < at);
  const next = delta > 0 ? (after[0] ?? marks[0]) : (before[before.length - 1] ?? marks[marks.length - 1]);
  view.dispatch({ selection: EditorSelection.range(next.from, next.to), scrollIntoView: true });
  return true;
}

const theme = EditorView.baseTheme({
  ".cm-usage-read": { backgroundColor: "var(--usage-read-bg)", borderRadius: "2px" },
  ".cm-usage-write": { backgroundColor: "var(--usage-write-bg)", borderRadius: "2px" },
});

/** The state, decorations and Escape (only while the sticky highlighting shows). */
export function usageHighlighting(): Extension {
  return [
    usageHighlightField,
    theme,
    Prec.high(
      keymap.of([
        {
          key: "Escape",
          run: (view) => {
            if (!hasStickyUsages(view.state)) return false;
            clearUsageHighlights(view);
            listeners.forEach((fn) => fn(view));
            return true;
          },
        },
      ]),
    ),
  ];
}
