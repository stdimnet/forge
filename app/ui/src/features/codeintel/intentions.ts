/**
 * Show Context Actions (Alt+Enter) like PHPStorm: the intention popup at the caret with
 * the language servers' quick fixes and refactorings and the PHP index's intentions
 * (`code_context_actions`, merged and ordered in Rust), and the light bulb that appears at
 * the caret line after a short pause when actions are available (red for quick fixes).
 * Choosing an entry applies its edit (`refactoring.ts`, one undo step) and moves the caret
 * where the entry says (Create method: into the new body, opening the class's file when it
 * is another one), runs its server command, the UI action it links to (Generate...) or the
 * handler of an action with arguments (Create class opens the Create New PHP Class dialog).
 */
import { createSignal } from "solid-js";
import { Lightbulb } from "lucide-solid";
import type { Extension } from "@codemirror/state";
import { EditorView, ViewPlugin, type ViewUpdate } from "@codemirror/view";
import { codeApi, isLibraryPath, type ContextAction, type ContextActionsAnswer, type MissingFeature } from "@/ipc/codeintel";
import { runAction, showBalloon } from "@/keymap";
import { showHint } from "@/features/lsp/hint";
import { syncNow } from "@/features/lsp/sync";
import { fromLsp, iconNode, toLsp } from "@/features/lsp/util";
import { openFile } from "@/state/editor";
import { visibleView } from "@/editor/views";
import { applyWorkspaceEdit, attachToRefactoring } from "./refactoring";
import { renameSessionOf } from "./rename";

export const NO_ACTIONS = "No context actions available at this location";

export interface IntentionPopupState {
  path: string;
  view: EditorView;
  actions: ContextAction[];
  missing?: MissingFeature | null;
  x: number;
  y: number;
}

const [intentionPopup, setIntentionPopup] = createSignal<IntentionPopupState | null>(null);
export { intentionPopup };

export function closeIntentionPopup(focus = true): void {
  const p = intentionPopup();
  setIntentionPopup(null);
  if (focus) p?.view.focus();
}

/**
 * Entries a UI feature adds for the caret, before the language servers' and the packs'
 * (the spelling and grammar fixes of `features/spelling`).
 */
export type ContextActionProvider = (path: string, view: EditorView) => Promise<ContextAction[]>;
const providers: { provide: ContextActionProvider; bulb: boolean }[] = [];

/**
 * `bulb: false`: the entries are offered on Alt+Enter but do not light the bulb (actions
 * available everywhere, such as "Explain with AI"); such providers are not asked for the bulb.
 */
export function registerContextActionProvider(provider: ContextActionProvider, opts: { bulb?: boolean } = {}): void {
  providers.push({ provide: provider, bulb: opts.bulb ?? true });
}

async function queryActions(path: string, view: EditorView, forBulb = false): Promise<ContextActionsAnswer> {
  const sel = view.state.selection.main;
  const doc = view.state.doc;
  const asked = providers.filter((p) => p.bulb || !forBulb);
  const extra = Promise.all(asked.map((p) => p.provide(path, view).catch(() => [] as ContextAction[])));
  let answer: ContextActionsAnswer;
  try {
    await syncNow(path);
    answer = await codeApi.contextActions(path, { start: toLsp(doc, sel.from), end: toLsp(doc, sel.to) }, doc.toString());
  } catch (e) {
    if (!(await extra).flat().length) throw e;
    answer = { actions: [] };
  }
  const added = (await extra).flat();
  if (!added.length) return answer;
  const fix = added.some((a) => a.kind === "quickFix" && !a.disabled);
  return { ...answer, actions: [...added, ...answer.actions], bulb: fix ? "fix" : (answer.bulb ?? "intention") };
}

/** Open the intention popup with the given entries at the caret (e.g. "Typo: Change to..." suggestions). */
export function showActionsPopup(path: string, view: EditorView, actions: ContextAction[]): void {
  const pos = view.state.selection.main.head;
  const line = view.state.doc.lineAt(pos);
  const coords = view.coordsAtPos(pos) ?? view.coordsAtPos(line.from);
  setIntentionPopup({ path, view, actions, x: Math.max(8, (coords?.left ?? 200) - 24), y: (coords?.bottom ?? 200) + 2 });
}

/** Alt+Enter: the intention popup below the caret. */
export async function showContextActions(path: string, view: EditorView): Promise<void> {
  const pos = view.state.selection.main.head;
  if (isLibraryPath(path)) {
    showHint(view, pos, NO_ACTIONS);
    return;
  }
  let answer: ContextActionsAnswer;
  try {
    answer = await queryActions(path, view);
  } catch (e) {
    showHint(view, pos, String(e instanceof Error ? e.message : e));
    return;
  }
  if (!answer.actions.length) {
    showHint(view, pos, answer.missing?.premium ? answer.missing.message : NO_ACTIONS);
    return;
  }
  const line = view.state.doc.lineAt(pos);
  const coords = view.coordsAtPos(pos) ?? view.coordsAtPos(line.from);
  setIntentionPopup({
    path,
    view,
    actions: answer.actions,
    missing: answer.missing,
    x: Math.max(8, (coords?.left ?? 200) - 24),
    y: (coords?.bottom ?? 200) + 2,
  });
}

export interface ContextActionContext {
  /** The file and editor Alt+Enter was pressed in. */
  path: string;
  view: EditorView;
}

type ContextActionHandler = (data: unknown, ctx: ContextActionContext) => void | Promise<void>;
const handlers = new Map<string, ContextActionHandler>();

/** Run entries whose `action` carries `data` (e.g. `PhpCreateClass`) with `run`. */
export function registerContextActionHandler(action: string, run: ContextActionHandler): void {
  handlers.set(action, run);
}

/** Move the caret (or select) where an applied entry says, opening its file if needed. */
async function placeCaret(caret: NonNullable<ContextAction["caret"]>, path: string, view: EditorView, refactoring?: number): Promise<void> {
  let target: EditorView | undefined = caret.path === path ? view : undefined;
  if (!target) {
    await openFile(caret.path);
    target = await visibleView(caret.path);
    if (target && refactoring) await attachToRefactoring(refactoring, caret.path);
  }
  if (!target) return;
  const doc = target.state.doc;
  const anchor = fromLsp(doc, caret.range.start);
  const head = fromLsp(doc, caret.range.end);
  target.dispatch({ selection: { anchor, head }, effects: EditorView.scrollIntoView(anchor, { y: "center" }) });
  target.focus();
}

/** Apply a chosen entry: its edit, its server command, or the action it links to. */
export async function runContextAction(path: string, view: EditorView, chosen: ContextAction): Promise<void> {
  closeIntentionPopup();
  if (chosen.disabled) {
    showHint(view, view.state.selection.main.head, chosen.disabled);
    return;
  }
  const handler = chosen.action && chosen.data != null ? handlers.get(chosen.action) : undefined;
  if (handler) {
    await handler(chosen.data, { path, view });
    return;
  }
  if (chosen.action) {
    await runAction(chosen.action);
    return;
  }
  try {
    let a = chosen;
    if (a.needsResolve && a.server) a = await codeApi.resolveAction(path, a.server, a.raw);
    const applied = a.edit && a.edit.changes.length ? await applyWorkspaceEdit(a.edit, { label: a.title }) : undefined;
    if (a.caret) await placeCaret(a.caret, path, view, applied?.id);
    const cmd = a.command;
    if (cmd && a.server) await codeApi.executeCommand(path, a.server, cmd.command, cmd.arguments);
  } catch (e) {
    showBalloon(`${chosen.title} failed`, String(e instanceof Error ? e.message : e), 6000);
  }
}

/** Icon class of an entry: red bulb for quick fixes, yellow for intentions, none for links. */
export function actionIcon(a: ContextAction): "fix" | "intention" | "refactor" | "source" | "link" {
  // Links open another popup or dialog and end with "..." ("Generate..."); other entries
  // that run a UI action ("Edit SQL Fragment", "Create class") are intentions or fixes.
  if (a.action && a.data == null && /(\.\.\.|\u2026)$/.test(a.title)) return "link";
  if (a.kind === "quickFix") return "fix";
  if (a.kind === "refactor") return "refactor";
  if (a.kind === "source") return "source";
  return "intention";
}

// ---------- light bulb ----------

/** PHPStorm shows the bulb after the caret rests for a moment. */
export const BULB_DELAY_MS = 500;

class BulbPlugin {
  dom: HTMLElement;
  timer: ReturnType<typeof setTimeout> | undefined;
  seq = 0;
  line = -1;
  kind: "fix" | "intention" | null = null;

  constructor(
    readonly view: EditorView,
    readonly path: string,
  ) {
    this.dom = document.createElement("button");
    this.dom.className = "cm-intention-bulb";
    this.dom.setAttribute("aria-label", "Show Context Actions");
    this.dom.title = "Show Context Actions";
    this.dom.append(iconNode(Lightbulb, 14));
    this.dom.style.display = "none";
    this.dom.addEventListener("mousedown", (e) => {
      e.preventDefault();
      void showContextActions(this.path, this.view);
    });
    view.scrollDOM.appendChild(this.dom);
    this.schedule();
  }

  update(u: ViewUpdate) {
    if (u.docChanged || u.selectionSet) {
      const line = u.state.doc.lineAt(u.state.selection.main.head).number;
      if (u.docChanged || line !== this.line) this.hide();
      this.schedule();
    } else if (u.geometryChanged && this.kind) this.place();
  }

  schedule() {
    clearTimeout(this.timer);
    const id = ++this.seq;
    this.timer = setTimeout(() => void this.query(id), BULB_DELAY_MS);
  }

  async query(id: number) {
    const view = this.view;
    if (!view.hasFocus || renameSessionOf(view) || intentionPopup()) return;
    const answer = await queryActions(this.path, view, true).catch(() => null);
    if (id !== this.seq || !answer?.bulb) {
      if (id === this.seq) this.hide();
      return;
    }
    this.kind = answer.bulb;
    this.line = view.state.doc.lineAt(view.state.selection.main.head).number;
    this.place();
  }

  place() {
    const view = this.view;
    if (!this.kind || this.line < 1 || this.line > view.state.doc.lines) return this.hide();
    const line = view.state.doc.line(this.line);
    view.requestMeasure({
      read: () => {
        const at = view.coordsAtPos(line.from);
        const box = view.scrollDOM.getBoundingClientRect();
        const content = view.contentDOM.getBoundingClientRect();
        return at && { top: at.top - box.top + view.scrollDOM.scrollTop, height: at.bottom - at.top, left: content.left - box.left + view.scrollDOM.scrollLeft };
      },
      write: (m) => {
        if (!m) return this.hide();
        // Inside the indentation when there is room, else just left of the text.
        const indent = /^\s*/.exec(line.text)?.[0].length ?? 0;
        const left = indent >= 2 ? m.left + 2 : m.left - 18;
        Object.assign(this.dom.style, { display: "", top: `${m.top + (m.height - 18) / 2}px`, left: `${Math.max(0, left)}px` });
        this.dom.classList.toggle("fix", this.kind === "fix");
      },
    });
  }

  hide() {
    this.kind = null;
    this.dom.style.display = "none";
  }

  destroy() {
    clearTimeout(this.timer);
    this.dom.remove();
  }
}

/** Editor extension: the light bulb at the caret line. */
export function intentionBulb(path: string): Extension {
  return ViewPlugin.define((view) => new BulbPlugin(view, path));
}
