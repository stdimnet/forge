/**
 * Go to Declaration or Usages: Ctrl+B (keymap action `GotoDeclaration`), Ctrl+Click and
 * middle click. The answer comes from Rust (`code_goto_declaration`: language servers, the
 * PHP index, the PHP stubs); on a declaration the usages are shown instead, like PHPStorm.
 * Holding Ctrl over a symbol underlines it like a link and shows where it is declared.
 * Ctrl+Alt+Click goes to the implementations.
 */
import { StateEffect, StateField, type Extension } from "@codemirror/state";
import { Decoration, EditorView, showTooltip, type DecorationSet, type Tooltip } from "@codemirror/view";
import { codeApi, type DeclarationAnswer, type SymbolHover } from "@/ipc/codeintel";
import { showUsagesAt } from "@/features/codeintel/usages";
import { chooseTargetAt } from "@/features/codeintel/targets";
import { hintField, showHint } from "./hint";
import { syncNow } from "./sync";
import { fromLsp, h, toLsp } from "./util";

interface Link {
  from: number;
  to: number;
  info?: { container: string; signature: string };
}

const setLink = StateEffect.define<Link | null>();

const linkField = StateField.define<Link | null>({
  create: () => null,
  update(link, tr) {
    for (const e of tr.effects) if (e.is(setLink)) return e.value;
    return tr.docChanged ? null : link;
  },
  provide: (f) => [
    EditorView.decorations.from(f, (link): DecorationSet =>
      link ? Decoration.set([Decoration.mark({ class: "cm-lsp-link" }).range(link.from, link.to)]) : Decoration.none,
    ),
    showTooltip.from(f, (link): Tooltip | null =>
      link?.info && (link.info.signature || link.info.container)
        ? {
            pos: link.from,
            above: true,
            create: () => ({
              dom: h(
                "div",
                "cm-goto-info",
                link.info!.container ? h("div", "cm-goto-info-container", link.info!.container) : null,
                link.info!.signature ? h("div", "cm-goto-info-signature", link.info!.signature) : null,
              ),
            }),
          }
        : null,
    ),
  ],
});

/**
 * Framework-specific targets (route names, service ids, templates, DQL fields) that a
 * language server cannot resolve. A provider returns true when it navigated.
 */
export type DeclarationProvider = (path: string, view: EditorView, pos: number) => Promise<boolean>;

const declarationProviders: DeclarationProvider[] = [];

export function registerDeclarationProvider(provider: DeclarationProvider): void {
  declarationProviders.push(provider);
}

const notFound = (message: string): DeclarationAnswer => ({ onDeclaration: false, targets: [], source: "none", message });

export async function gotoDeclaration(path: string, view: EditorView, pos = view.state.selection.main.head): Promise<boolean> {
  for (const provider of declarationProviders) {
    if (await provider(path, view, pos).catch(() => false)) return true;
  }
  await syncNow(path);
  const answer = await codeApi
    .gotoDeclaration(path, toLsp(view.state.doc, pos), view.state.doc.toString())
    .catch((e) => notFound(String(e instanceof Error ? e.message : e)));
  if (answer.onDeclaration) {
    await showUsagesAt(path, view, pos, { jumpIfSingle: true });
    return true;
  }
  if (!answer.targets.length) {
    showHint(view, pos, answer.message ?? "Cannot find declaration to go to");
    return false;
  }
  // Several definitions (an interface method, a class declared twice): let the user choose.
  chooseTargetAt(view, pos, answer.targets, answer.title ? `Choose Declaration of ${answer.title}` : undefined);
  return true;
}

/** Ctrl+Alt+Click: Go to Implementation (registered by the code intelligence feature). */
let implementationsAt: ((path: string, view: EditorView, pos: number) => Promise<void>) | null = null;
export function setImplementationNavigator(fn: typeof implementationsAt): void {
  implementationsAt = fn;
}

const isIdent = (c: string) => /[\w$\\]/.test(c);

function wordRange(view: EditorView, pos: number): { from: number; to: number } | null {
  const line = view.state.doc.lineAt(pos);
  const text = line.text;
  let s = pos - line.from;
  let e = s;
  while (s > 0 && isIdent(text[s - 1])) s--;
  while (e < text.length && isIdent(text[e])) e++;
  return s < e ? { from: line.from + s, to: line.from + e } : null;
}

export function lspGoto(path: string): Extension {
  let shown: Link | null = null;
  let seq = 0;
  let timer: ReturnType<typeof setTimeout> | undefined;
  const cache = new Map<string, Link | null>();
  let cachedDoc = "";
  const clear = (view: EditorView) => {
    seq++;
    clearTimeout(timer);
    if (!shown) return;
    shown = null;
    view.dispatch({ effects: setLink.of(null) });
  };
  const show = (view: EditorView, link: Link | null) => {
    if (!link) return clear(view);
    if (shown && shown.from === link.from && shown.to === link.to && shown.info === link.info) return;
    shown = link;
    view.dispatch({ effects: setLink.of(link) });
  };
  /** The navigable reference at `pos`, from Rust (cached per word while the text is unchanged). */
  const lookup = async (view: EditorView, pos: number): Promise<Link | null> => {
    const word = wordRange(view, pos);
    if (!word) return null;
    const doc = view.state.doc.toString();
    if (doc !== cachedDoc) {
      cache.clear();
      cachedDoc = doc;
    }
    const key = `${word.from}:${word.to}`;
    if (cache.has(key)) return cache.get(key)!;
    const answer: SymbolHover | null = await codeApi.symbolInfo(path, toLsp(view.state.doc, pos), doc).catch(() => null);
    const link: Link | null = answer
      ? {
          from: fromLsp(view.state.doc, answer.range.start),
          to: fromLsp(view.state.doc, answer.range.end),
          info: { container: answer.container, signature: answer.signature },
        }
      : // Languages without an index: underline the word for the language server.
        /\.(php|phtml|inc)$/i.test(path)
        ? null
        : word;
    cache.set(key, link);
    return link;
  };
  const navigate = (e: MouseEvent, view: EditorView) => {
    const pos = view.posAtCoords({ x: e.clientX, y: e.clientY });
    if (pos == null) return false;
    e.preventDefault();
    clear(view);
    view.dispatch({ selection: { anchor: pos } });
    if (e.altKey && implementationsAt) void implementationsAt(path, view, pos);
    else void gotoDeclaration(path, view, pos);
    return true;
  };
  return [
    linkField,
    hintField,
    EditorView.domEventHandlers({
      mousedown(e, view) {
        // Ctrl+Click (Cmd+Click on macOS) and middle click, like PHPStorm.
        if (e.button === 0 && (e.ctrlKey || e.metaKey)) return navigate(e, view);
        if (e.button === 1 && !e.ctrlKey && !e.metaKey && !e.altKey && !e.shiftKey) return navigate(e, view);
        return false;
      },
      mousemove(e, view) {
        if (!(e.ctrlKey || e.metaKey)) return clear(view), false;
        const pos = view.posAtCoords({ x: e.clientX, y: e.clientY });
        if (pos == null) return clear(view), false;
        if (shown && pos >= shown.from && pos <= shown.to) return false;
        const n = ++seq;
        clearTimeout(timer);
        timer = setTimeout(() => {
          void lookup(view, pos).then((link) => {
            if (n === seq) show(view, link);
          });
        }, 40);
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
