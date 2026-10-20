/**
 * Go to Declaration (Ctrl+B / Ctrl+Click) for Laravel symbols: route names → route
 * definition and controller, views, config/env keys, translations, columns, relations,
 * model properties, scopes, components and custom directives. Anything else falls through
 * to the language server. Several targets open a "Choose Declaration" list like PHPStorm.
 */
import { Prec, StateEffect, StateField, type Extension } from "@codemirror/state";
import { EditorView, keymap, showTooltip, type Tooltip } from "@codemirror/view";
import { laravelApi, type LaravelLocation } from "@/ipc/laravel";
import { gotoDeclaration } from "@/features/lsp/goto";
import { activeView } from "@/features/lsp/navigation";
import { commands, registerCommand } from "@/state/commands";
import { h } from "@/features/lsp/util";
import { laravelActive, reveal } from "./state";
import { isBlade } from "./completion";

/** Whether the caret may be on a Laravel symbol (checked before asking the backend). */
export function mayGoto(lineText: string, column: number, blade: boolean): boolean {
  const before = lineText.slice(0, column);
  const word = /[\w.:-]*$/.exec(before)![0];
  const head = before.slice(0, before.length - word.length);
  if (/[@\w]+\s*\([^()'"]*(?:\([^()]*\)[^()'"]*)*['"][^'"]*$/.test(before)) return true;
  if (/\[\s*\w+::class\s*,\s*['"][^'"]*$/.test(before)) return true;
  if (/\$\w+(?:\??->\w+)*\??->$/.test(head)) return true;
  if (/\b[A-Z]\w*::$/.test(head)) return true;
  if (blade && (/<\/?$/.test(head) || /@$/.test(head) || /<\/?(x-|livewire:)/.test(before))) return true;
  return false;
}

interface Chooser {
  pos: number;
  targets: LaravelLocation[];
  index: number;
}

const setChooser = StateEffect.define<Chooser | null>();

function chooserTooltip(c: Chooser): Tooltip {
  return {
    pos: c.pos,
    above: false,
    create: (view) => {
      const dom = h("div", "laravel-chooser");
      dom.append(h("div", "laravel-chooser-title", "Choose Declaration"));
      c.targets.forEach((t, i) => {
        const row = h("button", "laravel-chooser-row");
        const name = t.path.split("/").pop() ?? t.path;
        row.append(h("span", "laravel-chooser-name", `${name}:${t.line + 1}`), h("span", "laravel-chooser-path", t.path));
        row.onmousedown = (ev) => {
          ev.preventDefault();
          view.dispatch({ effects: setChooser.of(null) });
          void reveal(t);
        };
        if (i === c.index) row.classList.add("selected");
        dom.append(row);
      });
      return { dom };
    },
  };
}

const chooserField = StateField.define<Chooser | null>({
  create: () => null,
  update(value, tr) {
    for (const e of tr.effects) if (e.is(setChooser)) return e.value;
    return tr.docChanged || tr.selection ? null : value;
  },
  provide: (f) => showTooltip.from(f, (c) => (c ? chooserTooltip(c) : null)),
});

/** Arrow keys, Enter and Escape while the chooser is open. */
function chooserKey(key: "up" | "down" | "enter" | "escape") {
  return (view: EditorView): boolean => {
    const c = view.state.field(chooserField, false);
    if (!c) return false;
    if (key === "escape") view.dispatch({ effects: setChooser.of(null) });
    else if (key === "enter") {
      view.dispatch({ effects: setChooser.of(null) });
      void reveal(c.targets[c.index]);
    } else {
      const n = c.targets.length;
      const index = (c.index + (key === "down" ? 1 : n - 1)) % n;
      view.dispatch({ effects: setChooser.of({ ...c, index }) });
    }
    return true;
  };
}

/** Ask the backend; returns false when there is nothing Laravel-specific at `pos`. */
export async function laravelGoto(path: string, view: EditorView, pos: number): Promise<boolean> {
  const targets = await laravelApi.goto(path, pos, view.state.doc.toString()).catch(() => []);
  if (!targets.length) return false;
  if (targets.length === 1) {
    await reveal(targets[0]);
  } else {
    view.dispatch({ effects: setChooser.of({ pos, targets, index: 0 }) });
  }
  return true;
}

/** Laravel first, then the language server. */
async function gotoAt(path: string, view: EditorView, pos: number): Promise<void> {
  if (await laravelGoto(path, view, pos)) return;
  await gotoDeclaration(path, view, pos);
}

export function applies(path: string, view: EditorView, pos: number): boolean {
  if (!laravelActive()) return false;
  const line = view.state.doc.lineAt(pos);
  // Include the rest of the word so a caret at the start of a name still counts.
  const tail = /^[\w.:-]*/.exec(line.text.slice(pos - line.from))![0];
  return mayGoto(line.text, pos - line.from + Math.min(tail.length, 1), isBlade(path));
}

/**
 * Ctrl+B reaches the editor through the global keymap as the `GotoDeclaration` action.
 * Wrap the language server's command: Laravel symbols first, everything else unchanged.
 */
export function wrapGotoDeclaration(): void {
  const previous = commands().get("GotoDeclaration");
  registerCommand({
    id: "GotoDeclaration",
    title: previous?.title ?? "Go to Declaration or Usages",
    category: previous?.category ?? "Navigate",
    run: async () => {
      const a = activeView();
      if (a) {
        const pos = a.view.state.selection.main.head;
        if (applies(a.path, a.view, pos) && (await laravelGoto(a.path, a.view, pos))) return;
      }
      await previous?.run();
    },
  });
}

export function laravelGotoExtension(path: string): Extension {
  return [
    chooserField,
    Prec.high(
      keymap.of([
        {
          key: "Mod-b",
          run: (view) => {
            const pos = view.state.selection.main.head;
            if (!applies(path, view, pos)) return false;
            void gotoAt(path, view, pos);
            return true;
          },
        },
        { key: "Escape", run: chooserKey("escape") },
        { key: "Enter", run: chooserKey("enter") },
        { key: "ArrowDown", run: chooserKey("down") },
        { key: "ArrowUp", run: chooserKey("up") },
      ]),
    ),
    Prec.high(
      EditorView.domEventHandlers({
        mousedown(e, view) {
          if (e.button !== 0 || !(e.ctrlKey || e.metaKey)) return false;
          const pos = view.posAtCoords({ x: e.clientX, y: e.clientY });
          if (pos == null || !applies(path, view, pos)) return false;
          e.preventDefault();
          view.dispatch({ selection: { anchor: pos } });
          void gotoAt(path, view, pos);
          return true;
        },
      }),
    ),
  ];
}
