/**
 * Briefly highlight a line, e.g. after jumping to it from the browser or a link, so the eye
 * finds the caret. The extension is added to an editor on first use.
 */
import { StateEffect, StateField } from "@codemirror/state";
import { Decoration, EditorView, type DecorationSet } from "@codemirror/view";

const FLASH_MS = 1400;

const setFlash = StateEffect.define<number | null>();

const flashLine = Decoration.line({ class: "cm-forge-flash" });

const flashField = StateField.define<DecorationSet>({
  create: () => Decoration.none,
  update(deco, tr) {
    deco = deco.map(tr.changes);
    for (const e of tr.effects) {
      if (!e.is(setFlash)) continue;
      deco = e.value === null ? Decoration.none : Decoration.set([flashLine.range(e.value)]);
    }
    return deco;
  },
  provide: (f) => EditorView.decorations.from(f),
});

const flashTheme = EditorView.baseTheme({
  "@keyframes cm-forge-flash": {
    "0%, 35%": { backgroundColor: "var(--accent-soft)", boxShadow: "inset 2px 0 0 var(--accent)" },
    "100%": { backgroundColor: "transparent", boxShadow: "inset 2px 0 0 transparent" },
  },
  ".cm-line.cm-forge-flash": { animation: `cm-forge-flash ${FLASH_MS}ms ease-out forwards` },
});

const timers = new WeakMap<EditorView, ReturnType<typeof setTimeout>>();

/** Flash the line containing `pos`. */
export function flashLineAt(view: EditorView, pos: number): void {
  const from = view.state.doc.lineAt(pos).from;
  const effects: StateEffect<unknown>[] = [setFlash.of(from)];
  if (!view.state.field(flashField, false)) effects.unshift(StateEffect.appendConfig.of([flashField, flashTheme]));
  view.dispatch({ effects });
  clearTimeout(timers.get(view));
  timers.set(
    view,
    setTimeout(() => view.dispatch({ effects: setFlash.of(null) }), FLASH_MS),
  );
}
