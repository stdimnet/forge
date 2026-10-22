/**
 * The Vim emulation editor extension. Key presses in the editor go to the Rust engine
 * (`crates/forge-vim`, synchronous through WebAssembly) before CodeMirror's own keymaps; the
 * answer is applied as one transaction (edits, selection, mode), followed by its effects
 * (undo, IDE actions, scrolling). The IDE keymap leaves the keys Vim takes to the editor
 * (`claims` in the engine, asked by the keymap resolver), so no listener of our own is
 * needed outside CodeMirror.
 *
 * Rendering stays here: the block caret, search highlights, relative line numbers and the
 * command line panel.
 */
import {
  Annotation,
  ChangeSet,
  Compartment,
  EditorSelection,
  EditorState,
  Prec,
  StateEffect,
  StateField,
  Transaction,
  type Extension,
  type TransactionSpec,
} from "@codemirror/state";
import {
  Decoration,
  EditorView,
  ViewPlugin,
  WidgetType,
  lineNumbers,
  showPanel,
  type DecorationSet,
  type Panel,
  type ViewUpdate,
} from "@codemirror/view";
import { redo, undo } from "@codemirror/commands";
import { closeCompletion, completionStatus } from "@codemirror/autocomplete";
import { indentUnit } from "@codemirror/language";
import type { VimChange, VimEffect, VimMode, VimResponse, VimSync } from "@/ipc/vim";
import { vimEngine, vimReady } from "./engine";

/** What the extension needs from the rest of the IDE (set by the feature's index). */
export interface VimHost {
  enabled(): boolean;
  runAction(id: string): Promise<void>;
  isActionAvailable(id: string): boolean;
  openFile(path: string): Promise<void>;
  reloadRc(): Promise<void>;
  copy(text: string): void;
  /** A response of the editor `view` (for the status bar). */
  status(view: EditorView, res: VimResponse): void;
}

let host: VimHost = {
  enabled: () => false,
  runAction: async () => {},
  isActionAvailable: () => false,
  openFile: async () => {},
  reloadRc: async () => {},
  copy: () => {},
  status: () => {},
};

export function setVimHost(h: VimHost): void {
  host = h;
}

export interface VimViewState {
  active: boolean;
  mode: VimMode;
  /** Block caret (Visual mode; Normal mode uses the selection head). */
  cursor: number;
  cmdline: string | null;
  highlight: VimResponse["highlight"];
  number: boolean;
  relativenumber: boolean;
  scrolloff: number;
}

const initialState: VimViewState = {
  active: false,
  mode: "normal",
  cursor: 0,
  cmdline: null,
  highlight: null,
  number: true,
  relativenumber: false,
  scrolloff: 0,
};

const setVimState = StateEffect.define<Partial<VimViewState>>();

export const vimField = StateField.define<VimViewState>({
  create: () => initialState,
  update(value, tr) {
    let next = value;
    if (tr.docChanged) next = { ...next, cursor: tr.changes.mapPos(next.cursor) };
    for (const e of tr.effects) if (e.is(setVimState)) next = { ...next, ...e.value };
    return next;
  },
});

/** Marks the transactions that apply the engine's own edits (it knows them already). */
const vimEcho = Annotation.define<boolean>();

const clamp = (n: number, lo: number, hi: number) => Math.max(lo, Math.min(hi, n));

/** Fully visible lines (0-based) for H, M, L and scrolling. */
function visibleLines(view: EditorView): { top: number; bottom: number } {
  const doc = view.state.doc;
  const rect = view.scrollDOM.getBoundingClientRect();
  const visTop = rect.top - view.documentTop;
  const visBottom = rect.bottom - view.documentTop;
  if (!(rect.height > 0)) {
    const { from, to } = view.viewport;
    return { top: doc.lineAt(from).number - 1, bottom: doc.lineAt(to).number - 1 };
  }
  const topBlock = view.lineBlockAtHeight(Math.max(0, visTop));
  let top = doc.lineAt(topBlock.from).number - 1;
  if (topBlock.top < visTop - 2 && top + 1 < doc.lines) top++;
  const bottomBlock = view.lineBlockAtHeight(Math.max(0, visBottom - 1));
  let bottom = doc.lineAt(bottomBlock.from).number - 1;
  if (bottomBlock.bottom > visBottom + 2 && bottom > top) bottom--;
  return { top, bottom };
}

const controllers = new Set<VimController>();
/** The handle the engine considers focused. */
let focusedHandle: number | null = null;

class VimController {
  handle: number | null = null;
  private pending: VimChange[][] = [];
  private needText = false;
  private timer: ReturnType<typeof setTimeout> | undefined;

  constructor(readonly view: EditorView) {
    controllers.add(this);
    queueMicrotask(() => this.refresh());
  }

  get active(): boolean {
    return this.handle != null;
  }

  /** Attach to or detach from the engine as the emulation is turned on or off. */
  refresh(): void {
    const want = host.enabled() && vimReady();
    if (want && this.handle == null) {
      this.handle = vimEngine.attach(this.view.state.doc.toString());
      this.pending = [];
      this.needText = false;
      if (this.view.hasFocus) this.setFocus(true);
      this.apply(vimEngine.state(this.handle));
    } else if (!want && this.handle != null) {
      this.setFocus(false);
      vimEngine.detach(this.handle);
      this.handle = null;
      clearTimeout(this.timer);
      this.view.dispatch({
        effects: [setVimState.of({ ...initialState }), lineNumbersCompartment.reconfigure([])],
      });
    }
  }

  setFocus(focused: boolean): void {
    if (this.handle == null) return;
    if (focused) {
      focusedHandle = this.handle;
      vimEngine.focus(this.handle);
      host.status(this.view, vimEngine.state(this.handle));
    } else if (focusedHandle === this.handle) {
      focusedHandle = null;
      vimEngine.focus(null);
    }
  }

  update(u: ViewUpdate): void {
    if (u.focusChanged) this.setFocus(u.view.hasFocus);
    if (this.handle == null) return;
    for (const tr of u.transactions) {
      if (!tr.docChanged || tr.annotation(vimEcho)) continue;
      const batch: VimChange[] = [];
      tr.changes.iterChanges((fromA, toA, _fromB, _toB, inserted) =>
        batch.push({ from: fromA, to: toA, insert: inserted.toString() }),
      );
      this.pending.push(batch);
    }
    // Many edits (a reload, a long paste session): send the text instead.
    if (this.pending.length > 500) {
      this.pending = [];
      this.needText = true;
    }
  }

  destroy(): void {
    controllers.delete(this);
    clearTimeout(this.timer);
    if (this.handle != null) {
      this.setFocus(false);
      vimEngine.detach(this.handle);
      this.handle = null;
    }
  }

  private sync(): VimSync {
    const s = this.view.state;
    const sync: VimSync = {
      selection: s.selection.ranges.map((r) => ({ anchor: r.anchor, head: r.head })),
      main: s.selection.mainIndex,
      viewport: visibleLines(this.view),
      indentUnit: s.facet(indentUnit),
    };
    if (this.needText) {
      sync.text = s.doc.toString();
      this.needText = false;
      this.pending = [];
    } else if (this.pending.length) {
      sync.changes = this.pending;
      this.pending = [];
    }
    return sync;
  }

  /** Handle a key press; true when Vim took it. */
  key(e: KeyboardEvent): boolean {
    if (this.handle == null) return false;
    clearTimeout(this.timer);
    let res: VimResponse;
    try {
      const { key, code, ctrlKey, altKey, shiftKey, metaKey } = e;
      res = vimEngine.key(this.handle, { key, code, ctrlKey, altKey, shiftKey, metaKey }, this.sync());
    } catch (err) {
      console.warn("vim:", err);
      this.needText = true;
      return false;
    }
    this.apply(res);
    return res.consumed;
  }

  private timeout(): void {
    if (this.handle == null) return;
    this.apply(vimEngine.timeout(this.handle, this.sync()));
  }

  apply(res: VimResponse): void {
    const view = this.view;
    const state = view.state;
    let length = state.doc.length;
    const spec: TransactionSpec = { annotations: [vimEcho.of(true)] };
    if (res.changes.length) {
      let cs = ChangeSet.empty(length);
      for (const c of res.changes) {
        const len = cs.newLength;
        const from = clamp(c.from, 0, len);
        cs = cs.compose(ChangeSet.of({ from, to: clamp(c.to, from, len), insert: c.insert }, len));
      }
      spec.changes = cs;
      length = cs.newLength;
      spec.annotations = [vimEcho.of(true), Transaction.userEvent.of("vim.edit")];
    }
    if (res.selection?.length) {
      const ranges = res.selection.map((r) => EditorSelection.range(clamp(r.anchor, 0, length), clamp(r.head, 0, length)));
      spec.selection = EditorSelection.create(ranges, clamp(res.main, 0, ranges.length - 1));
    }
    const prev = state.field(vimField, false) ?? initialState;
    const next: VimViewState = {
      active: true,
      mode: res.mode,
      cursor: clamp(res.cursor, 0, length),
      cmdline: res.cmdline,
      highlight: res.highlight,
      number: res.number,
      relativenumber: res.relativenumber,
      scrolloff: res.scrolloff,
    };
    const effects: StateEffect<unknown>[] = [setVimState.of(next)];
    if (prev.relativenumber !== next.relativenumber || prev.number !== next.number || !prev.active) {
      effects.push(lineNumbersCompartment.reconfigure(next.relativenumber ? relativeLineNumbers(next.number) : []));
    }
    spec.effects = effects;
    view.dispatch(spec);
    if (view.state.doc.length !== res.docLength) this.needText = true;
    host.status(view, res);
    if (res.wait && res.timeoutlen > 0) this.timer = setTimeout(() => this.timeout(), res.timeoutlen);
    if (res.effects.length || res.resume) void this.runEffects(res);
  }

  private async runEffects(res: VimResponse): Promise<void> {
    for (const ef of res.effects) {
      try {
        await this.runEffect(ef, res);
      } catch (err) {
        console.warn("vim effect failed", ef, err);
      }
    }
    if (res.resume && this.handle != null) this.apply(vimEngine.key(this.handle, null, this.sync()));
  }

  private async runEffect(ef: VimEffect, res: VimResponse): Promise<void> {
    const view = this.view;
    const len = () => view.state.doc.length;
    switch (ef.kind) {
      case "undo":
        for (let i = 0; i < ef.count; i++) undo(view);
        break;
      case "redo":
        for (let i = 0; i < ef.count; i++) redo(view);
        break;
      case "action":
        await host.runAction(ef.id);
        break;
      case "actionOnRange": {
        view.dispatch({ selection: { anchor: clamp(ef.from, 0, len()), head: clamp(ef.to, 0, len()) } });
        await host.runAction(host.isActionAvailable(ef.id) ? ef.id : "ReformatCode");
        view.dispatch({ selection: { anchor: clamp(ef.caret, 0, len()) } });
        break;
      }
      case "open":
        await host.openFile(ef.path);
        break;
      case "clipboard":
        host.copy(ef.text);
        break;
      case "reveal": {
        const margin = ef.align === "nearest" ? Math.max(5, res.scrolloff * view.defaultLineHeight) : 0;
        view.dispatch({ effects: EditorView.scrollIntoView(clamp(ef.pos, 0, len()), { y: ef.align, yMargin: margin }) });
        break;
      }
      case "scrollTo": {
        const line = view.state.doc.line(clamp(ef.line + 1, 1, view.state.doc.lines));
        view.dispatch({ effects: EditorView.scrollIntoView(line.from, { y: "start", yMargin: 0 }) });
        break;
      }
      case "reloadRc":
        await host.reloadRc();
        break;
      case "bell":
        view.dom.classList.add("cm-vim-bell");
        setTimeout(() => view.dom.classList.remove("cm-vim-bell"), 150);
        break;
    }
  }
}

const vimPlugin = ViewPlugin.fromClass(VimController);

/** Attach or detach every open editor (the emulation was turned on or off). */
export function refreshVimEditors(): void {
  for (const c of controllers) c.refresh();
}

/** The controller of a view (for tests and the status bar). */
export const vimControllerOf = (view: EditorView): VimController | null => view.plugin(vimPlugin);

// ---- Keys

const keyHandler = Prec.highest(
  EditorView.domEventHandlers({
    keydown(e, view) {
      const ctl = view.plugin(vimPlugin);
      if (!ctl?.active || e.isComposing || e.keyCode === 229) return false;
      // Escape closes the completion popup and leaves Insert mode, like IdeaVim.
      if (e.key === "Escape" && completionStatus(view.state) != null) closeCompletion(view);
      if (!ctl.key(e)) return false;
      e.preventDefault();
      return true;
    },
    paste(e) {
      const text = e.clipboardData?.getData("text/plain");
      if (text) vimEngine.setClipboard(text);
      return false;
    },
  }),
);

// ---- Block caret

class BlockCaretWidget extends WidgetType {
  toDOM(): HTMLElement {
    const span = document.createElement("span");
    span.className = "cm-vim-cursor cm-vim-cursor-eol";
    span.textContent = " ";
    return span;
  }
  eq(): boolean {
    return true;
  }
  ignoreEvent(): boolean {
    return true;
  }
}

function caretDecorations(view: EditorView): DecorationSet {
  const st = view.state.field(vimField, false);
  if (!st?.active || st.mode === "insert") return Decoration.none;
  const doc = view.state.doc;
  const pos = clamp(st.mode.startsWith("visual") ? st.cursor : view.state.selection.main.head, 0, doc.length);
  const line = doc.lineAt(pos);
  const cls = st.mode === "replace" ? "cm-vim-cursor cm-vim-cursor-replace" : "cm-vim-cursor";
  if (pos < line.to) {
    const code = doc.sliceString(pos, pos + 1).charCodeAt(0);
    const end = code >= 0xd800 && code <= 0xdbff ? pos + 2 : pos + 1;
    return Decoration.set([Decoration.mark({ class: cls }).range(pos, Math.min(end, line.to))]);
  }
  return Decoration.set([Decoration.widget({ widget: new BlockCaretWidget(), side: 1 }).range(pos)]);
}

const caretPlugin = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet;
    constructor(view: EditorView) {
      this.decorations = caretDecorations(view);
    }
    update(u: ViewUpdate) {
      if (u.docChanged || u.selectionSet || u.startState.field(vimField, false) !== u.state.field(vimField, false)) {
        this.decorations = caretDecorations(u.view);
      }
    }
  },
  { decorations: (v) => v.decorations },
);

// ---- Search highlights (hlsearch, incsearch)

const searchMark = Decoration.mark({ class: "cm-vim-search" });

function searchDecorations(view: EditorView): DecorationSet {
  const hl = view.state.field(vimField, false)?.highlight;
  if (!hl?.source) return Decoration.none;
  let re: RegExp;
  try {
    re = new RegExp(hl.source, hl.ignoreCase ? "gim" : "gm");
  } catch {
    return Decoration.none;
  }
  const marks = [];
  for (const { from, to } of view.visibleRanges) {
    const start = view.state.doc.lineAt(from).from;
    const end = view.state.doc.lineAt(to).to;
    const text = view.state.sliceDoc(start, end);
    re.lastIndex = 0;
    let m: RegExpExecArray | null;
    let count = 0;
    while ((m = re.exec(text)) && count++ < 2000) {
      if (m[0].length === 0) {
        re.lastIndex++;
        continue;
      }
      marks.push(searchMark.range(start + m.index, start + m.index + m[0].length));
    }
  }
  return Decoration.set(marks, true);
}

const searchPlugin = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet;
    constructor(view: EditorView) {
      this.decorations = searchDecorations(view);
    }
    update(u: ViewUpdate) {
      const changed = u.startState.field(vimField, false)?.highlight !== u.state.field(vimField, false)?.highlight;
      if (changed || u.docChanged || u.viewportChanged) this.decorations = searchDecorations(u.view);
    }
  },
  { decorations: (v) => v.decorations },
);

// ---- Line numbers (relativenumber)

const lineNumbersCompartment = new Compartment();

function relativeLineNumbers(absoluteCurrent: boolean): Extension {
  return lineNumbers({
    formatNumber: (n, state) => {
      const cur = state.doc.lineAt(state.selection.main.head).number;
      if (n === cur) return absoluteCurrent ? String(n) : "0";
      return String(Math.abs(n - cur));
    },
  });
}

/** Relative numbers follow the caret line. */
const relativeNumberRefresh = EditorState.transactionExtender.of((tr) => {
  const st = tr.startState.field(vimField, false);
  if (!st?.active || !st.relativenumber || !tr.selection) return null;
  const before = tr.startState.doc.lineAt(tr.startState.selection.main.head).number;
  const after = tr.newDoc.lineAt(tr.newSelection.main.head).number;
  return before === after ? null : { effects: lineNumbersCompartment.reconfigure(relativeLineNumbers(st.number)) };
});

// ---- Command line panel

function cmdlinePanel(view: EditorView): Panel {
  const dom = document.createElement("div");
  dom.className = "cm-vim-cmdline";
  const text = document.createElement("span");
  const caret = document.createElement("span");
  caret.className = "cm-vim-cmdline-caret";
  caret.textContent = " ";
  dom.append(text, caret);
  const render = (state: EditorState) => (text.textContent = state.field(vimField, false)?.cmdline ?? "");
  render(view.state);
  return { dom, update: (u) => render(u.state) };
}

const cmdlinePanelExt = showPanel.compute([vimField], (state) => {
  const st = state.field(vimField, false);
  return st?.active && st.cmdline != null ? cmdlinePanel : null;
});

const editorClass = EditorView.editorAttributes.compute([vimField], (state) => {
  const st = state.field(vimField, false);
  if (!st?.active) return {} as Record<string, string>;
  return { class: `cm-vim cm-vim-${st.mode}${st.number || st.relativenumber ? "" : " cm-vim-nonumber"}` };
});

/** The Vim extension of an editor; inactive until the emulation is turned on. */
export function vimExtension(): Extension {
  return [
    vimField,
    vimPlugin,
    keyHandler,
    caretPlugin,
    searchPlugin,
    lineNumbersCompartment.of([]),
    relativeNumberRefresh,
    cmdlinePanelExt,
    editorClass,
  ];
}
