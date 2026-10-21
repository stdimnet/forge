/**
 * Edit Fragment: the injected text of a string in its own editor, docked below the host
 * editor like PHPStorm's fragment editor. Edits flow both ways: typing in the fragment
 * replaces the matching source text (escaped for the literal by Rust, `injections_escape`),
 * and changes to the string in the host editor refresh the fragment. The value-to-source
 * offset map comes from Rust (`injections_fragment`).
 */
import { Annotation, EditorState, StateEffect, StateField, type Extension } from "@codemirror/state";
import {
  EditorView,
  drawSelection,
  highlightActiveLine,
  highlightActiveLineGutter,
  highlightSpecialChars,
  keymap,
  lineNumbers,
  showPanel,
  type Panel,
  type ViewUpdate,
} from "@codemirror/view";
import { defaultKeymap, history, historyKeymap, indentWithTab } from "@codemirror/commands";
import { bracketMatching } from "@codemirror/language";
import { X } from "lucide-solid";
import { forgeEditorTheme, forgeHighlight } from "@/editor/forgeTheme";
import { iconNode } from "@/features/lsp/util";
import { injectionsApi, type Fragment } from "@/ipc/injections";
import { injectionSettings } from "./highlight";
import { fragmentLanguageSupport } from "./languages";

interface OpenFragment {
  path: string;
  /** A source offset inside the literal (its content start), mapped through edits. */
  anchor: number;
}

export const openFragment = StateEffect.define<OpenFragment | null>();

/** Changes made by the fragment editor (the host must not echo them back). */
const fromFragment = Annotation.define<boolean>();
/** Changes the host made to the fragment editor. */
const fromHost = Annotation.define<boolean>();

/** The fragment of the literal at `anchor` in the current host text. */
export function fragmentAt(state: EditorState, path: string, anchor: number): Fragment | null {
  try {
    return injectionsApi.fragmentSync({ text: state.doc.toString(), path, offset: anchor, ...injectionSettings() }) ?? null;
  } catch {
    return null;
  }
}

const fileName = (path: string) => path.split(/[\\/]/).pop() ?? path;

/** Inner view of the panel, for the Ctrl+Enter action (Run SQL of the fragment). */
export function fragmentHostOf(el: Element | null): { host: EditorView; fragment: () => Fragment | null } | null {
  const panel = el?.closest<HTMLElement>(".cm-fragment-panel");
  return panel ? (panelHosts.get(panel) ?? null) : null;
}

const panelHosts = new WeakMap<HTMLElement, { host: EditorView; fragment: () => Fragment | null }>();

function createPanel(host: EditorView): Panel {
  const dom = document.createElement("div");
  dom.className = "cm-fragment-panel";
  dom.setAttribute("aria-label", "Fragment editor");
  const header = document.createElement("div");
  header.className = "cm-fragment-header";
  const title = document.createElement("span");
  title.className = "cm-fragment-title";
  const close = document.createElement("button");
  close.className = "icon-btn cm-fragment-close";
  close.title = "Close";
  close.setAttribute("aria-label", "Close");
  close.append(iconNode(X, 14));
  close.addEventListener("mousedown", (e) => {
    e.preventDefault();
    host.dispatch({ effects: openFragment.of(null) });
    host.focus();
  });
  header.append(title, close);
  const body = document.createElement("div");
  body.className = "cm-fragment-body";
  dom.append(header, body);

  let open = host.state.field(fragmentField)!;
  let current = fragmentAt(host.state, open.path, open.anchor);
  let language = current?.language ?? "";

  const setTitle = () => {
    title.textContent = current ? `${current.label} Fragment (${fileName(open.path)})` : "Fragment";
  };
  setTitle();

  /** Typing in the fragment: replace the source text behind each change, escaped. */
  const syncOut = (u: ViewUpdate) => {
    if (!current || !u.docChanged || u.transactions.every((t) => t.annotation(fromHost))) return;
    const frag = current;
    const changes: { from: number; to: number; insert: string }[] = [];
    u.changes.iterChanges((fromA, toA, _fromB, _toB, inserted) => {
      const text = inserted.toString();
      const insert = text ? (injectionsApi.escapeSync(frag.kind, text) ?? text) : "";
      changes.push({ from: frag.map[fromA] ?? frag.to, to: frag.map[toA] ?? frag.to, insert });
    });
    host.dispatch({ changes, annotations: fromFragment.of(true) });
  };

  const inner = new EditorView({
    parent: body,
    state: EditorState.create({
      doc: current?.text ?? "",
      extensions: innerExtensions(language, syncOut),
    }),
  });
  panelHosts.set(dom, { host, fragment: () => current });

  /** Re-read the fragment after host changes; show its text when it differs. */
  const refresh = () => {
    open = host.state.field(fragmentField) ?? open;
    const next = fragmentAt(host.state, open.path, open.anchor);
    if (!next) {
      // The string or its injection is gone.
      queueMicrotask(() => host.dispatch({ effects: openFragment.of(null) }));
      return;
    }
    current = next;
    if (next.language !== language) {
      language = next.language;
      inner.setState(EditorState.create({ doc: next.text, extensions: innerExtensions(language, syncOut) }));
    } else if (inner.state.doc.toString() !== next.text) {
      inner.dispatch({
        changes: { from: 0, to: inner.state.doc.length, insert: next.text },
        annotations: fromHost.of(true),
      });
    }
    setTitle();
  };

  queueMicrotask(() => inner.focus());

  return {
    dom,
    update(u: ViewUpdate) {
      if (u.docChanged || u.startState.field(fragmentField) !== u.state.field(fragmentField)) refresh();
    },
    destroy() {
      inner.destroy();
    },
  };
}

function innerExtensions(language: string, syncOut: (u: ViewUpdate) => void): Extension {
  return [
    lineNumbers(),
    highlightActiveLineGutter(),
    highlightSpecialChars(),
    history(),
    drawSelection(),
    highlightActiveLine(),
    bracketMatching(),
    keymap.of([...defaultKeymap, ...historyKeymap, indentWithTab]),
    forgeEditorTheme,
    forgeHighlight,
    fragmentLanguageSupport(language),
    EditorView.lineWrapping,
    EditorView.updateListener.of(syncOut),
  ];
}

const fragmentField = StateField.define<OpenFragment | null>({
  create: () => null,
  update(value, tr) {
    for (const e of tr.effects) if (e.is(openFragment)) return e.value;
    if (value && tr.docChanged) return { ...value, anchor: tr.changes.mapPos(value.anchor, -1) };
    return value;
  },
  // A constant constructor: the panel stays while the anchor moves.
  provide: (f) => showPanel.from(f, (v) => (v ? createPanel : null)),
});

/** The Edit Fragment panel of a host editor. */
export function fragmentEditor(): Extension {
  return fragmentField;
}

/** Open the fragment editor for the injected string at the caret; false when there is none. */
export function editFragment(view: EditorView, path: string): boolean {
  const pos = view.state.selection.main.head;
  const frag = fragmentAt(view.state, path, pos);
  if (!frag) return false;
  view.dispatch({ effects: openFragment.of({ path, anchor: frag.from }) });
  return true;
}
