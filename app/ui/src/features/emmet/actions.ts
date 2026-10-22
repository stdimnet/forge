/**
 * Emmet actions: Surround with Emmet (also the "Emmet" entry of Surround with Live Template,
 * Ctrl+Alt+J), Update Tag with Emmet, Remove Tag and Go to Next/Previous Emmet Edit Point
 * (Alt+Shift+] / Alt+Shift+[). Rust computes the result (`emmet_*`); these show and apply it.
 */
import { EditorSelection } from "@codemirror/state";
import type { EditorView } from "@codemirror/view";
import { activeView } from "@/editor/views";
import { editor } from "@/state/editor";
import { languageIdFor } from "@/editor/languages";
import { showBalloon } from "@/keymap/dispatch";
import { emmetApi } from "@/ipc/emmet";
import { applyEdits, applyExpansion, emmetOptions, markupEnabled } from "./model";
import { openAbbreviationPopup } from "./AbbreviationPopup";

/** The active file editor, if any. */
function target(): { path: string; view: EditorView } | null {
  const view = activeView();
  const path = editor.active;
  return view && path ? { path, view } : null;
}

const message = (e: unknown) => (e instanceof Error ? e.message : String(e));

function wrapRequest(path: string, view: EditorView, abbreviation: string) {
  const { from, to } = view.state.selection.main;
  return { text: view.state.doc.toString(), from, to, abbreviation, languageId: languageIdFor(path), options: emmetOptions(view) };
}

/** Surround the selection (or the current line) with an abbreviation. */
export function surroundWithEmmet(t = target()): void {
  if (!t) return;
  const { path, view } = t;
  if (!markupEnabled(languageIdFor(path))) {
    showBalloon("Emmet is not available in this file");
    return;
  }
  openAbbreviationPopup(view, {
    title: "Surround with Emmet",
    preview: (abbr) => {
      const e = emmetApi.wrapSync(wrapRequest(path, view, abbr));
      return e ? { text: e.text } : null;
    },
    submit: async (abbr) => {
      try {
        applyExpansion(view, await emmetApi.wrap(wrapRequest(path, view, abbr)));
      } catch (e) {
        showBalloon("Invalid abbreviation", message(e));
        view.focus();
      }
    },
  });
}

/** The "Emmet" entry of Surround with Live Template in markup files. */
export function emmetSurroundItem(path: string, view: EditorView) {
  if (!markupEnabled(languageIdFor(path))) return null;
  return {
    id: "emmet",
    label: "Emmet",
    detail: "Surround with Emmet abbreviation",
    run: () => surroundWithEmmet({ path, view }),
  };
}

function tagRequest(view: EditorView, abbreviation = "") {
  return { text: view.state.doc.toString(), offset: view.state.selection.main.head, abbreviation, options: emmetOptions(view) };
}

/** Rename the tag around the caret and add classes, id and attributes from an abbreviation. */
export function updateTag(): void {
  const t = target();
  if (!t) return;
  const { view } = t;
  openAbbreviationPopup(view, {
    title: "Update Tag with Emmet",
    submit: async (abbr) => {
      try {
        applyEdits(view, await emmetApi.updateTag(tagRequest(view, abbr)));
      } catch (e) {
        showBalloon(message(e));
        view.focus();
      }
    },
  });
}

/** Remove the tag around the caret, keeping its content. */
export async function removeTag(): Promise<void> {
  const t = target();
  if (!t) return;
  try {
    applyEdits(t.view, await emmetApi.removeTag(tagRequest(t.view)));
  } catch (e) {
    showBalloon(message(e));
  }
}

/** Go to the next or previous edit point: empty attribute values, empty tags, empty lines. */
export async function goToEditPoint(forward: boolean): Promise<void> {
  const t = target();
  if (!t) return;
  const { view } = t;
  const offset = await emmetApi.editPoint({ text: view.state.doc.toString(), offset: view.state.selection.main.head, forward });
  if (offset == null) return;
  view.dispatch({ selection: EditorSelection.cursor(offset), scrollIntoView: true });
  view.focus();
}
