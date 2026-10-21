/**
 * The popups of the documentation actions, shown at the caret like PHPStorm: Quick
 * Documentation, Quick Definition, Type Info, Context Info and Error Description. One popup
 * at a time; it closes when the caret moves, on typing and on Escape.
 */
import { Prec, StateEffect, StateField, type Extension } from "@codemirror/state";
import { EditorView, keymap, showTooltip, type Tooltip } from "@codemirror/view";

export type PopupKind = "doc" | "definition" | "info";

export interface DocPopup {
  kind: PopupKind;
  pos: number;
  /** Above the line (small hints) or below it (documentation, definitions). */
  above: boolean;
  build: (view: EditorView) => HTMLElement;
}

const setPopup = StateEffect.define<DocPopup | null>();

const popupField = StateField.define<DocPopup | null>({
  create: () => null,
  update(popup, tr) {
    for (const e of tr.effects) if (e.is(setPopup)) return e.value;
    return tr.selection || tr.docChanged ? null : popup;
  },
  provide: (f) =>
    showTooltip.from(f, (p): Tooltip | null =>
      p
        ? {
            pos: p.pos,
            above: p.above,
            create: (view) => {
              const dom = p.build(view);
              dom.classList.add("doc-popup", `doc-popup-${p.kind}`);
              return { dom };
            },
          }
        : null,
    ),
});

const popupKeys = Prec.highest(
  keymap.of([
    {
      key: "Escape",
      run: (view) => {
        if (!view.state.field(popupField, false)) return false;
        view.dispatch({ effects: setPopup.of(null) });
        return true;
      },
    },
  ]),
);

/** Editor extension: the documentation popups. */
export function docPopups(): Extension {
  return [popupField, popupKeys];
}

export const popupOf = (view: EditorView): DocPopup | null => view.state.field(popupField, false) ?? null;

export function showPopup(view: EditorView, popup: DocPopup): void {
  if (view.state.field(popupField, false) === undefined) return;
  view.dispatch({ effects: setPopup.of(popup) });
}

export function closePopup(view: EditorView): void {
  if (view.state.field(popupField, false)) view.dispatch({ effects: setPopup.of(null) });
}
