/**
 * Caret row highlighting (Settings | Editor | General | Appearance, "Highlight caret row"):
 * the lines of every caret, like CodeMirror's `highlightActiveLine`, but not the lines of a
 * caret with a selection, as in PHPStorm. Otherwise the row color would hide the selections
 * of multiple carets.
 */
import type { Range } from "@codemirror/state";
import { Decoration, ViewPlugin, type DecorationSet, type EditorView, type ViewUpdate } from "@codemirror/view";

const caretRow = Decoration.line({ class: "cm-activeLine" });

function rows(view: EditorView): DecorationSet {
  const deco: Range<Decoration>[] = [];
  let last = -1;
  for (const r of view.state.selection.ranges) {
    if (!r.empty) continue;
    const line = view.lineBlockAt(r.head);
    if (line.from > last) {
      deco.push(caretRow.range(line.from));
      last = line.from;
    }
  }
  return Decoration.set(deco, true);
}

export const highlightCaretRows = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet;
    constructor(view: EditorView) {
      this.decorations = rows(view);
    }
    update(u: ViewUpdate) {
      if (u.docChanged || u.selectionSet) this.decorations = rows(u.view);
    }
  },
  { decorations: (v) => v.decorations },
);
