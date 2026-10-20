/**
 * Go to Declaration: Ctrl+B and Ctrl+Click. Holding Ctrl over a symbol underlines it like
 * a link, as in PHPStorm.
 */
import { StateEffect, StateField, type Extension } from "@codemirror/state";
import { Decoration, EditorView, keymap, showTooltip, type DecorationSet, type Tooltip } from "@codemirror/view";
import { lspApi } from "@/ipc/lsp";
import { revealLocation } from "./navigation";
import { syncNow } from "./sync";
import { h, toLsp } from "./util";

const setLink = StateEffect.define<{ from: number; to: number } | null>();

const linkField = StateField.define<DecorationSet>({
  create: () => Decoration.none,
  update(deco, tr) {
    for (const e of tr.effects) {
      if (e.is(setLink)) {
        return e.value ? Decoration.set([Decoration.mark({ class: "cm-lsp-link" }).range(e.value.from, e.value.to)]) : Decoration.none;
      }
    }
    return tr.docChanged ? deco.map(tr.changes) : deco;
  },
  provide: (f) => EditorView.decorations.from(f),
});

/** Short-lived hint like "Cannot find declaration to go to". */
const setHint = StateEffect.define<{ pos: number; text: string } | null>();
const hintField = StateField.define<Tooltip | null>({
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

function showHint(view: EditorView, pos: number, text: string) {
  view.dispatch({ effects: setHint.of({ pos, text }) });
  setTimeout(() => {
    if (view.state.field(hintField, false)) view.dispatch({ effects: setHint.of(null) });
  }, 2500);
}

/**
 * Framework-specific targets (route names, service ids, templates, DQL fields) that a
 * language server cannot resolve. A provider returns true when it navigated.
 */
export type DeclarationProvider = (path: string, view: EditorView, pos: number) => Promise<boolean>;

const declarationProviders: DeclarationProvider[] = [];

export function registerDeclarationProvider(provider: DeclarationProvider): void {
  declarationProviders.push(provider);
}

export async function gotoDeclaration(path: string, view: EditorView, pos = view.state.selection.main.head): Promise<boolean> {
  for (const provider of declarationProviders) {
    if (await provider(path, view, pos).catch(() => false)) return true;
  }
  await syncNow(path);
  const locations = await lspApi.definition(path, toLsp(view.state.doc, pos)).catch(() => []);
  const target = locations[0];
  if (!target) {
    showHint(view, pos, "Cannot find declaration to go to");
    return false;
  }
  return revealLocation(target.path, target.range.start, target.range.end);
}

function linkRangeAt(view: EditorView, x: number, y: number) {
  const pos = view.posAtCoords({ x, y });
  if (pos == null) return null;
  const word = view.state.wordAt(pos);
  return word && word.to > word.from ? word : null;
}

export function lspGoto(path: string): Extension {
  let linked: { from: number; to: number } | null = null;
  const clear = (view: EditorView) => {
    if (!linked) return;
    linked = null;
    view.dispatch({ effects: setLink.of(null) });
  };
  return [
    linkField,
    hintField,
    keymap.of([{ key: "Mod-b", run: (view) => (void gotoDeclaration(path, view), true) }]),
    EditorView.domEventHandlers({
      mousedown(e, view) {
        if (e.button !== 0 || !(e.ctrlKey || e.metaKey)) return false;
        const pos = view.posAtCoords({ x: e.clientX, y: e.clientY });
        if (pos == null) return false;
        e.preventDefault();
        clear(view);
        void gotoDeclaration(path, view, pos);
        return true;
      },
      mousemove(e, view) {
        if (!(e.ctrlKey || e.metaKey)) return clear(view), false;
        const range = linkRangeAt(view, e.clientX, e.clientY);
        if (!range) return clear(view), false;
        if (linked && linked.from === range.from && linked.to === range.to) return false;
        linked = range;
        view.dispatch({ effects: setLink.of(range) });
        return false;
      },
      keyup(e, view) {
        if (e.key === "Control" || e.key === "Meta") clear(view);
        return false;
      },
      mouseleave(_e, view) {
        clear(view);
        return false;
      },
    }),
  ];
}
