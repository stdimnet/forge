/**
 * Bookmark gutter icons. Positions are mapped through every edit right away (so icons move
 * with the text while typing) and each edit is reported to the store as line edits; the
 * Rust model then moves the stored lines the same way and the gutter is rebuilt from it.
 */
import { createEffect, createRoot, on } from "solid-js";
import { RangeSet, StateEffect, StateField, type Text } from "@codemirror/state";
import { EditorView, GutterMarker, ViewPlugin, gutter } from "@codemirror/view";
import { registerEditorExtension } from "@/editor/extensions";
import { isSyncedTransaction } from "@/editor/docSync";
import type { Bookmark, LineEdit } from "@/ipc/workbench";
import { bookmarks, bookmarksIn, hasPendingEdits, queueEdits } from "./store";

const BOOKMARK_SVG =
  '<svg width="12" height="12" viewBox="0 0 24 24" fill="currentColor" stroke="currentColor" stroke-width="2" stroke-linejoin="round"><path d="m19 21-7-4-7 4V5a2 2 0 0 1 2-2h10a2 2 0 0 1 2 2v16z"/></svg>';

class BookmarkMarker extends GutterMarker {
  constructor(readonly bookmark: Bookmark) {
    super();
  }

  eq(other: GutterMarker): boolean {
    return (
      other instanceof BookmarkMarker &&
      other.bookmark.mnemonic === this.bookmark.mnemonic &&
      other.bookmark.description === this.bookmark.description
    );
  }

  toDOM(): Node {
    const el = document.createElement("span");
    const b = this.bookmark;
    el.className = b.mnemonic ? "cm-bookmark cm-bookmark-mnemonic" : "cm-bookmark";
    el.title = `Bookmark${b.mnemonic ? ` ${b.mnemonic}` : ""}${b.description ? `: ${b.description}` : ""}`;
    if (b.mnemonic) el.textContent = b.mnemonic;
    else el.innerHTML = BOOKMARK_SVG;
    return el;
  }
}

const setMarks = StateEffect.define<RangeSet<GutterMarker>>();

function build(doc: Text, list: Bookmark[]): RangeSet<GutterMarker> {
  const ranges = list
    .filter((b) => b.line < doc.lines)
    .sort((a, b) => a.line - b.line)
    .map((b) => new BookmarkMarker(b).range(doc.line(b.line + 1).from));
  return RangeSet.of(ranges, true);
}

const marksField = StateField.define<RangeSet<GutterMarker>>({
  create: () => RangeSet.empty,
  update(value, tr) {
    for (const e of tr.effects) if (e.is(setMarks)) return e.value;
    return tr.docChanged ? value.map(tr.changes) : value;
  },
});

/** The line edits of one transaction (old-document coordinates, ascending). */
export function lineEditsOf(startDoc: Text, changes: { iterChanges: (f: (fromA: number, toA: number, fromB: number, toB: number, inserted: Text) => void) => void }): LineEdit[] {
  const edits: LineEdit[] = [];
  changes.iterChanges((fromA, toA, _fromB, _toB, inserted) => {
    const start = startDoc.lineAt(fromA);
    const removed = startDoc.lineAt(toA).number - start.number;
    const added = inserted.lines - 1;
    if (removed || added) edits.push({ line: start.number - 1, removed, added, atLineStart: fromA === start.from });
  });
  return edits;
}

registerEditorExtension(({ path }) => [
  marksField,
  gutter({
    class: "cm-bookmark-gutter",
    markers: (v) => v.state.field(marksField),
  }),
  EditorView.updateListener.of((u) => {
    for (const tr of u.transactions) {
      if (!tr.docChanged || isSyncedTransaction(tr)) continue;
      // A reload replaces the whole text; it says nothing about where lines went.
      let whole = false;
      tr.changes.iterChanges((fromA, toA) => (whole ||= fromA === 0 && toA === tr.startState.doc.length && toA > 0));
      if (!whole) queueEdits(path, lineEditsOf(tr.startState.doc, tr.changes));
    }
  }),
  ViewPlugin.define((view) => {
    // Rebuild the icons from the store whenever it changes (unless edits are on their way).
    let destroyed = false;
    const dispose = createRoot((dispose) => {
      createEffect(
        on(bookmarks, () => {
          if (hasPendingEdits(path)) return;
          const marks = build(view.state.doc, bookmarksIn(path));
          queueMicrotask(() => destroyed || view.dispatch({ effects: setMarks.of(marks) }));
        }),
      );
      return dispose;
    });
    return {
      destroy: () => {
        destroyed = true;
        dispose();
      },
    };
  }),
]);
