/**
 * Short-lived hints above the caret, like PHPStorm's "Cannot find declaration to go to" or
 * "No usages found in Project Files". Part of every editor (see `lspGoto`).
 */
import { StateEffect, StateField } from "@codemirror/state";
import { EditorView, showTooltip, type Tooltip } from "@codemirror/view";
import { h } from "./util";

const setHint = StateEffect.define<{ pos: number; text: string } | null>();

export const hintField = StateField.define<Tooltip | null>({
  create: () => null,
  update(tip, tr) {
    for (const e of tr.effects) {
      if (e.is(setHint)) {
        return e.value
          ? {
              pos: e.value.pos,
              above: true,
              create: () => ({ dom: h("div", "cm-lsp-hint", e.value!.text) }),
            }
          : null;
      }
    }
    return tr.selection || tr.docChanged ? null : tip;
  },
  provide: (f) => showTooltip.from(f),
});

export function showHint(view: EditorView, pos: number, text: string): void {
  if (!view.state.field(hintField, false)) return;
  view.dispatch({ effects: setHint.of({ pos, text }) });
  setTimeout(() => {
    if (view.state.field(hintField, false)) view.dispatch({ effects: setHint.of(null) });
  }, 2500);
}
