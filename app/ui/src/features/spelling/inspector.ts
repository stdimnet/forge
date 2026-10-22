/**
 * The "Typo" and "Grammar" inspections of an editor. Files up to `WHOLE_FILE_LIMIT`
 * characters are checked once completely; larger ones as their visible part scrolls into
 * view. After an edit only the changed lines (grown to the comment or string around them)
 * are checked again, a moment after typing stops, so typing stays fast. Problems are kept
 * in document offsets (mapped through edits) and published to the shared diagnostics store
 * as source `spelling`, which draws the squiggles and fills the Problems tool window.
 */
import { syntaxTree } from "@codemirror/language";
import type { EditorState, Extension } from "@codemirror/state";
import { ViewPlugin, type EditorView, type PluginValue, type ViewUpdate } from "@codemirror/view";
import type { EditorDiagnostic, Severity } from "@/ipc/lsp";
import { spellApi, type SpellProblem } from "@/ipc/spell";
import { fileTypeOf } from "@/editor/fileTypes";
import { publishDiagnostics } from "@/features/lsp/store";
import { toLsp } from "@/features/lsp/util";
import { collectSegments, expandRange, isProseFile, mergeRanges } from "./segments";
import { dictionaryWords, inspectionsOn, problemSeverity, spellOptions } from "./options";

/** Larger files are checked as they scroll into view. */
export const WHOLE_FILE_LIMIT = 200_000;
/** Pause after typing before the changed lines are checked again. */
export const EDIT_DELAY_MS = 400;

export const SPELLING_SOURCE = "spelling";

type Range = [number, number];

const live = new Map<string, Set<SpellInspector>>();
const byView = new WeakMap<EditorView, SpellInspector>();

const baseName = (path: string) => path.slice(path.lastIndexOf("/") + 1);

export class SpellInspector implements PluginValue {
  problems: SpellProblem[] = [];
  private fileProblems: SpellProblem[] = [];
  private checked: Range[] = [];
  private dirty: Range[] = [];
  private inflight: Range[] = [];
  private fileNameDirty = true;
  private timer: ReturnType<typeof setTimeout> | undefined;
  private running = false;
  private destroyed = false;

  constructor(
    readonly view: EditorView,
    readonly path: string,
  ) {
    byView.set(view, this);
    let set = live.get(path);
    if (!set) live.set(path, (set = new Set()));
    set.add(this);
    this.invalidate();
  }

  private get whole(): boolean {
    return this.view.state.doc.length <= WHOLE_FILE_LIMIT;
  }

  private visible(): Range[] {
    return this.view.visibleRanges.map((r) => [r.from, r.to] as Range);
  }

  /** Check everything again (settings or dictionaries changed). */
  invalidate(): void {
    this.problems = [];
    this.checked = [];
    this.fileNameDirty = true;
    this.dirty = this.whole ? [[0, this.view.state.doc.length]] : this.visible();
    this.schedule(30);
  }

  update(u: ViewUpdate): void {
    if (u.docChanged) {
      const map = (r: Range): Range => [u.changes.mapPos(r[0], -1), u.changes.mapPos(r[1], 1)];
      this.problems = this.problems
        .map((p) => ({ ...p, from: u.changes.mapPos(p.from, 1), to: u.changes.mapPos(p.to, -1) }))
        .filter((p) => p.to > p.from);
      this.dirty = this.dirty.map(map);
      this.inflight = this.inflight.map(map);
      this.checked = this.checked.map(map);
      u.changes.iterChangedRanges((_fa, _ta, fb, tb) => this.dirty.push([fb, tb]));
    }
    const treeGrew = !u.docChanged && syntaxTree(u.startState) !== syntaxTree(u.state);
    if (treeGrew) {
      // The parser got further (large files parse in the background): check what is on screen.
      this.dirty.push(...this.visible());
    } else if (!this.whole && (u.viewportChanged || u.docChanged)) {
      for (const r of this.visible()) if (!this.checked.some((c) => c[0] <= r[0] && c[1] >= r[1])) this.dirty.push(r);
    }
    if (this.dirty.length) this.schedule(u.docChanged ? EDIT_DELAY_MS : 150);
  }

  private schedule(delay: number): void {
    clearTimeout(this.timer);
    this.timer = setTimeout(() => void this.run(), delay);
  }

  private async run(): Promise<void> {
    if (this.destroyed) return;
    if (this.running) {
      this.schedule(EDIT_DELAY_MS);
      return;
    }
    if (!inspectionsOn()) {
      this.dirty = [];
      if (this.problems.length || this.fileProblems.length) {
        this.problems = [];
        this.fileProblems = [];
        this.publish();
      }
      return;
    }
    const state = this.view.state;
    const ranges = mergeRanges(this.dirty.map(([f, t]) => expandRange(state, f, t)));
    this.dirty = [];
    this.inflight = ranges;
    const checkName = this.fileNameDirty;
    this.fileNameDirty = false;
    this.running = true;
    try {
      const options = spellOptions();
      const words = dictionaryWords();
      const prose = isProseFile(fileTypeOf(this.path).id);
      const segments = collectSegments(state, ranges, prose);
      const doc = state.doc;
      const [found, names] = await Promise.all([
        segments.length ? spellApi.inspect({ segments, options, words }) : Promise.resolve([]),
        checkName && options.fileNames
          ? spellApi.inspect({ segments: [{ from: 0, text: baseName(this.path), kind: "fileName" }], options, words })
          : Promise.resolve(null),
      ]);
      if (this.destroyed) return;
      if (names) this.fileProblems = names;
      else if (checkName) this.fileProblems = [];
      if (!this.view.state.doc.eq(doc)) {
        // Edited meanwhile: check the (mapped) ranges again with the new text.
        this.dirty.push(...this.inflight);
      } else {
        const inside = (p: SpellProblem) => ranges.some(([f, t]) => p.from >= f && p.to <= t);
        this.problems = [...this.problems.filter((p) => !inside(p)), ...found].sort((a, b) => a.from - b.from);
        this.checked = mergeRanges([...this.checked, ...ranges]);
      }
      this.publish();
    } catch {
      // The next edit tries again.
    } finally {
      this.running = false;
      this.inflight = [];
      if (this.dirty.length) this.schedule(EDIT_DELAY_MS);
    }
  }

  /** Problems whose range touches `pos`. */
  problemsAt(pos: number): SpellProblem[] {
    return this.problems.filter((p) => p.from <= pos && p.to >= pos);
  }

  publish(): void {
    const set = live.get(this.path);
    // With split editors, the most recent view of the file speaks for it.
    if (set && [...set].pop() !== this) return;
    publishDiagnostics(this.path, toDiagnostics(this.view.state, this.problems, this.fileProblems), SPELLING_SOURCE);
  }

  destroy(): void {
    this.destroyed = true;
    clearTimeout(this.timer);
    const set = live.get(this.path);
    set?.delete(this);
    if (!set?.size) {
      live.delete(this.path);
      publishDiagnostics(this.path, [], SPELLING_SOURCE);
    } else {
      [...set].pop()?.publish();
    }
  }
}

export function toDiagnostics(state: EditorState, problems: SpellProblem[], fileProblems: SpellProblem[] = []): EditorDiagnostic[] {
  const doc = state.doc;
  const diag = (p: SpellProblem, fileLevel: boolean): EditorDiagnostic => ({
    range: fileLevel
      ? { start: { line: 0, character: 0 }, end: { line: 0, character: 0 } }
      : { start: toLsp(doc, p.from), end: toLsp(doc, p.to) },
    severity: problemSeverity(p.kind) as Severity,
    message: p.message,
    source: p.kind === "typo" ? "Typo" : "Grammar",
    fileLevel: fileLevel || undefined,
    data: p,
  });
  return [...fileProblems.map((p) => diag(p, true)), ...problems.map((p) => diag(p, false))];
}

/** The inspector of an editor. */
export const inspectorOf = (view: EditorView): SpellInspector | undefined => byView.get(view);

/** Check every open editor again (settings or dictionaries changed). */
export function invalidateAll(): void {
  for (const set of live.values()) for (const i of set) i.invalidate();
}

export function spellInspection(path: string): Extension {
  return ViewPlugin.define((view) => new SpellInspector(view, path));
}
