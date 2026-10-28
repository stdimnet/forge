/**
 * Inline (ghost text) completion in the editor, like JetBrains AI Assistant: after typing
 * pauses (`ai.completion.debounceMs`) the backend asks the Code completion model
 * (`ai_inline_complete`; context, prompt and clean-up in Rust, `forge-ai-editor`) and the
 * answer is shown as gray text at the caret. Typing on cancels the request; typing what the
 * suggestion says keeps the rest.
 *
 * Keys are keymap actions (rebindable in Settings | Keymap): Insert Inline Proposal (Tab),
 * Insert Inline Proposal Word (Ctrl+Right), Insert Inline Proposal Line, Next / Previous
 * Proposal (Alt+] / Alt+[), Call Inline Completion, and Escape (EditorEscape) dismisses.
 * They are "native" actions: the editor handles them only while a suggestion is shown, so
 * Tab indents and Ctrl+Right moves by word otherwise.
 */
import { createSignal } from "solid-js";
import { completionStatus, hasNextSnippetField } from "@codemirror/autocomplete";
import { Prec, StateEffect, StateField, type Extension, type Transaction } from "@codemirror/state";
import { Decoration, EditorView, ViewPlugin, WidgetType, type ViewUpdate } from "@codemirror/view";
import { aiEditorApi, type RelatedFile } from "@/ipc/aiEditor";
import { shortcutsOf, strokeOfEvent } from "@/keymap";
import { editor } from "@/state/editor";
import { getSetting } from "@/state/settings";

export const INSERT = "InsertInlineCompletionAction";
export const INSERT_WORD = "InsertInlineCompletionWordAction";
export const INSERT_LINE = "InsertInlineCompletionLineAction";
export const NEXT = "NextInlineCompletionAction";
export const PREV = "PrevInlineCompletionAction";
export const CALL = "CallInlineCompletionAction";
export const ESCAPE = "EditorEscape";

/** What the status bar widget shows. */
export type InlineActivity = "idle" | "working" | "error";
const [activity, setActivity] = createSignal<InlineActivity>("idle");
const [lastError, setLastError] = createSignal<string | null>(null);
export { activity as inlineActivity, lastError as inlineError };

export interface Suggestion {
  /** Where the ghost text starts. */
  pos: number;
  text: string;
  /** Alternatives asked with Next Proposal; `index` is the shown one. */
  alternatives: string[];
  index: number;
}

export const setSuggestion = StateEffect.define<Suggestion | null>();

/** The part of `text` Insert Inline Proposal Word takes: leading space plus one word or symbol run. */
export function nextWord(text: string): string {
  const m = /^(\s*)([\p{L}\p{N}_$]+|[^\p{L}\p{N}_$\s]+)?/u.exec(text);
  const word = m ? m[0] : "";
  return word || text.slice(0, 1);
}

/** The part Insert Inline Proposal Line takes: up to the end of the current line (or the next one). */
export function nextLine(text: string): string {
  const nl = text.indexOf("\n");
  if (nl < 0) return text;
  if (nl === 0) {
    const second = text.indexOf("\n", 1);
    return second < 0 ? text : text.slice(0, second);
  }
  return text.slice(0, nl);
}

/** The suggestion after the user typed `typed` at its start; null when it no longer matches. */
export function typedThrough(s: Suggestion, at: number, typed: string): Suggestion | null {
  if (at !== s.pos || !typed || !s.text.startsWith(typed) || typed.length >= s.text.length) return null;
  return { ...s, pos: s.pos + typed.length, text: s.text.slice(typed.length), alternatives: [s.text.slice(typed.length)], index: 0 };
}

function typedText(tr: Transaction): { at: number; text: string } | null {
  let out: { at: number; text: string } | null = null;
  let count = 0;
  tr.changes.iterChanges((fromA, toA, _fromB, _toB, inserted) => {
    count++;
    if (fromA === toA) out = { at: fromA, text: inserted.toString() };
  });
  return count === 1 ? out : null;
}

export const suggestionField = StateField.define<Suggestion | null>({
  create: () => null,
  update(value, tr) {
    for (const e of tr.effects) if (e.is(setSuggestion)) return e.value;
    if (!value) return null;
    if (tr.docChanged) {
      const typed = tr.isUserEvent("input.type") ? typedText(tr) : null;
      return typed ? typedThrough(value, typed.at, typed.text) : null;
    }
    if (tr.selection && tr.state.selection.main.head !== value.pos) return null;
    return value;
  },
  provide: (f) =>
    EditorView.decorations.from(f, (s) =>
      s ? Decoration.set([Decoration.widget({ widget: new GhostText(s.text), side: 1 }).range(s.pos)]) : Decoration.none,
    ),
});

class GhostText extends WidgetType {
  constructor(readonly text: string) {
    super();
  }
  eq(other: GhostText) {
    return other.text === this.text;
  }
  toDOM() {
    const span = document.createElement("span");
    span.className = "cm-ai-ghost";
    span.textContent = this.text;
    span.setAttribute("aria-label", `AI suggestion: ${this.text}`);
    return span;
  }
  ignoreEvent() {
    return false;
  }
}

/** Insert `part` of the shown suggestion; the rest stays shown. */
function acceptPart(view: EditorView, part: string): boolean {
  const s = view.state.field(suggestionField, false);
  if (!s || !part) return false;
  const rest = s.text.slice(part.length);
  view.dispatch({
    changes: { from: s.pos, insert: part },
    selection: { anchor: s.pos + part.length },
    effects: setSuggestion.of(rest ? { pos: s.pos + part.length, text: rest, alternatives: [rest], index: 0 } : null),
    userEvent: "input.ai",
    scrollIntoView: true,
  });
  return true;
}

export const acceptSuggestion = (view: EditorView) => acceptPart(view, view.state.field(suggestionField, false)?.text ?? "");
export const acceptWord = (view: EditorView) => acceptPart(view, nextWord(view.state.field(suggestionField, false)?.text ?? ""));
export const acceptLine = (view: EditorView) => acceptPart(view, nextLine(view.state.field(suggestionField, false)?.text ?? ""));

export function dismissSuggestion(view: EditorView): boolean {
  if (!view.state.field(suggestionField, false)) return false;
  view.dispatch({ effects: setSuggestion.of(null) });
  return true;
}

/** The most recent other open files, as context for the model. */
function relatedFiles(path: string): RelatedFile[] {
  return editor.tabs
    .filter((t) => t.path !== path && !t.view && t.content)
    .slice(-6)
    .reverse()
    .map((t) => ({ path: t.path, text: t.content.slice(0, 4000) }));
}

let nextId = 0;

class InlinePlugin {
  readonly id = `inline-${++nextId}`;
  timer: ReturnType<typeof setTimeout> | undefined;
  seq = 0;
  inflight = false;

  constructor(
    readonly view: EditorView,
    readonly path: string,
  ) {}

  update(u: ViewUpdate) {
    const typed = u.transactions.some(
      (tr) => tr.docChanged && tr.isUserEvent("input") && !tr.isUserEvent("input.paste") && !tr.isUserEvent("input.complete") && !tr.isUserEvent("input.ai") && !tr.isUserEvent("input.drop"),
    );
    if (typed) {
      if (u.state.field(suggestionField, false)) this.cancel();
      else this.schedule();
    } else if (u.docChanged || (u.selectionSet && !u.state.field(suggestionField, false))) {
      this.cancel();
    }
  }

  cancel() {
    clearTimeout(this.timer);
    this.seq++;
    if (this.inflight) {
      this.inflight = false;
      setActivity("idle");
      void aiEditorApi.inlineCancel(this.id).catch(() => undefined);
    }
  }

  schedule() {
    this.cancel();
    if (!getSetting<boolean>("ai.completion.enabled", false)) return;
    const delay = Math.max(50, getSetting<number>("ai.completion.debounceMs", 350));
    this.timer = setTimeout(() => void this.request(), delay);
  }

  /** Whether a suggestion makes sense here (one empty caret, no popup, no snippet, focused). */
  applicable(): boolean {
    const st = this.view.state;
    return st.selection.ranges.length === 1 && st.selection.main.empty && completionStatus(st) == null && !hasNextSnippetField(st) && !st.readOnly;
  }

  async request(temperature?: number): Promise<string | null> {
    if (!this.applicable()) return null;
    const seq = ++this.seq;
    const st = this.view.state;
    const offset = st.selection.main.head;
    this.inflight = true;
    setActivity("working");
    try {
      const answer = await aiEditorApi.inlineComplete({ path: this.path, text: st.doc.toString(), offset, related: relatedFiles(this.path) }, this.id, temperature);
      if (seq !== this.seq) return null;
      this.inflight = false;
      setActivity("idle");
      setLastError(null);
      if (this.view.state.doc !== st.doc || this.view.state.selection.main.head !== offset) return null;
      return answer.text ?? null;
    } catch (e) {
      if (seq === this.seq) {
        this.inflight = false;
        setActivity("error");
        setLastError(String(e instanceof Error ? e.message : e));
      }
      return null;
    }
  }

  /** Ask now and show the answer (debounced typing and Call Inline Completion). */
  async show(temperature?: number) {
    const offset = this.view.state.selection.main.head;
    const text = await this.request(temperature);
    if (text) this.view.dispatch({ effects: setSuggestion.of({ pos: offset, text, alternatives: [text], index: 0 }) });
  }

  /** Next / Previous Proposal: cycle, asking the model for a new alternative at the end. */
  async cycle(dir: 1 | -1) {
    const s = this.view.state.field(suggestionField, false);
    if (!s) return void this.show();
    const i = s.index + dir;
    if (i >= 0 && i < s.alternatives.length) {
      this.view.dispatch({ effects: setSuggestion.of({ ...s, index: i, text: s.alternatives[i] }) });
      return;
    }
    if (dir < 0) {
      const last = s.alternatives.length - 1;
      this.view.dispatch({ effects: setSuggestion.of({ ...s, index: last, text: s.alternatives[last] }) });
      return;
    }
    const text = await this.request(Math.min(1, 0.2 * s.alternatives.length + 0.2));
    const now = this.view.state.field(suggestionField, false);
    if (!text || !now || now.pos !== s.pos) return;
    const alternatives = now.alternatives.includes(text) ? now.alternatives : [...now.alternatives, text];
    const index = alternatives.indexOf(text);
    this.view.dispatch({ effects: setSuggestion.of({ ...now, alternatives, index, text }) });
  }

  destroy() {
    this.cancel();
  }
}

const plugins = new WeakMap<EditorView, InlinePlugin>();

/** Call Inline Completion / Next / Previous Proposal for `view`. */
export function callInlineCompletion(view: EditorView): void {
  void plugins.get(view)?.show();
}

export function cycleSuggestion(view: EditorView, dir: 1 | -1): void {
  void plugins.get(view)?.cycle(dir);
}

/** Run the keymap action bound to `e` while a suggestion is shown. */
function onKey(e: KeyboardEvent, view: EditorView): boolean {
  const s = view.state.field(suggestionField, false);
  const stroke = strokeOfEvent(e);
  if (!stroke) return false;
  const bound = (id: string) => shortcutsOf(id).includes(stroke);
  if (!s) {
    if (bound(CALL)) {
      callInlineCompletion(view);
      return true;
    }
    return false;
  }
  if (completionStatus(view.state) != null) return false;
  if (bound(INSERT)) return acceptSuggestion(view);
  if (bound(INSERT_WORD)) return acceptWord(view);
  if (bound(INSERT_LINE)) return acceptLine(view);
  if (bound(NEXT) || bound(PREV)) {
    cycleSuggestion(view, bound(NEXT) ? 1 : -1);
    return true;
  }
  if (bound(ESCAPE)) return dismissSuggestion(view);
  return false;
}

/** Editor extension: ghost text, requests and the accept keys. */
export function inlineCompletion(path: string): Extension {
  return [
    suggestionField,
    ViewPlugin.define((view) => {
      const p = new InlinePlugin(view, path);
      plugins.set(view, p);
      return p;
    }),
    Prec.highest(
      EditorView.domEventHandlers({
        keydown(e, view) {
          if (!onKey(e, view)) return false;
          e.preventDefault();
          return true;
        },
      }),
    ),
  ];
}
