/**
 * PHPStorm editor actions implemented on top of CodeMirror (duplicate line, move line,
 * comment, extend selection, folding, find, ...). Language-aware actions (reformat,
 * refactorings) are left to language packs.
 */
import { EditorSelection, type SelectionRange } from "@codemirror/state";
import { EditorView, ViewPlugin, type Command as CmCommand } from "@codemirror/view";
import {
  copyLineDown,
  cursorMatchingBracket,
  deleteLine,
  indentSelection,
  insertBlankLine,
  moveLineDown,
  moveLineUp,
  selectParentSyntax,
  splitLine,
  toggleBlockComment,
  toggleComment,
} from "@codemirror/commands";
import { foldAll, foldCode, unfoldAll, unfoldCode } from "@codemirror/language";
import {
  findNext,
  findPrevious,
  gotoLine,
  selectNextOccurrence,
  selectSelectionMatches,
} from "@codemirror/search";
import { startCompletion } from "@codemirror/autocomplete";
import { registerEditorExtension } from "@/editor/extensions";
import { editor } from "@/state/editor";
import { viewFor as sharedViewFor } from "@/editor/views";
import { gotoHighlightedUsage } from "@/editor/usageHighlight";
import { openFind, openReplace } from "@/features/find/findBar/commands";

const views = new Map<string, EditorView>();

registerEditorExtension(({ path }) =>
  ViewPlugin.define((view) => {
    views.set(path, view);
    return {
      destroy: () => {
        if (views.get(path) === view) views.delete(path);
      },
    };
  }),
);

/** The CodeMirror view of the active editor tab, if any. */
export const activeView = (): EditorView | undefined => (editor.active ? sharedViewFor(editor.active) : undefined);

/** Selection history for Extend/Shrink Selection (Ctrl+W / Ctrl+Shift+W). */
const selectionStack = new WeakMap<EditorView, { before: EditorSelection; after: EditorSelection }[]>();

const extendSelection: CmCommand = (view) => {
  const before = view.state.selection;
  const empty = before.ranges.every((r) => r.empty);
  // First press on an empty caret selects the word, like PHPStorm.
  const ok = empty ? selectWord(view) || selectParentSyntax(view) : selectParentSyntax(view);
  if (ok) {
    const stack = selectionStack.get(view) ?? [];
    stack.push({ before, after: view.state.selection });
    selectionStack.set(view, stack);
  }
  return ok;
};

const shrinkSelection: CmCommand = (view) => {
  const stack = selectionStack.get(view);
  const top = stack?.[stack.length - 1];
  if (!top || !top.after.eq(view.state.selection)) {
    selectionStack.delete(view);
    return false;
  }
  stack!.pop();
  view.dispatch({ selection: top.before, scrollIntoView: true });
  return true;
};

function selectWord(view: EditorView): boolean {
  const ranges = view.state.selection.ranges.map((r) => view.state.wordAt(r.head) ?? r);
  if (ranges.every((r, i) => r.eq(view.state.selection.ranges[i]))) return false;
  view.dispatch({ selection: EditorSelection.create(ranges, view.state.selection.mainIndex) });
  return true;
}

const duplicate: CmCommand = (view) => {
  const { state } = view;
  if (state.selection.ranges.every((r) => r.empty)) return copyLineDown(view);
  const changes = state.selection.ranges.map((r) => ({ from: r.to, insert: state.sliceDoc(r.from, r.to) }));
  const ranges = state.selection.ranges.map((r) => EditorSelection.range(r.to, r.to + (r.to - r.from)));
  view.dispatch(state.update({ changes, selection: EditorSelection.create(ranges), scrollIntoView: true, userEvent: "input.copyline" }));
  return true;
};

const joinLines: CmCommand = (view) => {
  const { state } = view;
  const changes = state.selection.ranges.flatMap((r) => {
    const first = state.doc.lineAt(r.from);
    const last = state.doc.lineAt(r.to);
    const end = last.number === first.number ? first.number + 1 : last.number;
    const out = [];
    for (let n = first.number; n < end && n < state.doc.lines; n++) {
      const line = state.doc.line(n);
      const next = state.doc.line(n + 1);
      const lead = /^\s*/.exec(next.text)![0].length;
      const trail = line.text.length - line.text.trimEnd().length;
      out.push({ from: line.to - trail, to: next.from + lead, insert: " " });
    }
    return out;
  });
  if (!changes.length) return false;
  view.dispatch({ changes, userEvent: "input.joinlines" });
  return true;
};

const toggleCase: CmCommand = (view) => {
  const { state } = view;
  const ranges = state.selection.ranges.map((r) => (r.empty ? (state.wordAt(r.head) ?? r) : r));
  const changes = ranges
    .filter((r) => !r.empty)
    .map((r) => {
      const text = state.sliceDoc(r.from, r.to);
      return { from: r.from, to: r.to, insert: text === text.toUpperCase() ? text.toLowerCase() : text.toUpperCase() };
    });
  if (!changes.length) return false;
  view.dispatch({ changes, selection: EditorSelection.create(ranges as SelectionRange[]), userEvent: "input.case" });
  return true;
};

const startNewLineBefore: CmCommand = (view) => {
  const { state } = view;
  const line = state.doc.lineAt(state.selection.main.head);
  const indent = /^\s*/.exec(line.text)![0];
  view.dispatch({
    changes: { from: line.from, insert: `${indent}\n` },
    selection: { anchor: line.from + indent.length },
    scrollIntoView: true,
    userEvent: "input",
  });
  return true;
};

const unselectOccurrence: CmCommand = (view) => {
  const sel = view.state.selection;
  if (sel.ranges.length < 2) return false;
  const ranges = sel.ranges.filter((_, i) => i !== sel.mainIndex);
  view.dispatch({ selection: EditorSelection.create(ranges, Math.max(0, sel.mainIndex - 1)), scrollIntoView: true });
  return true;
};

/** Select All Occurrences: without a selection, of the word at the caret (PHPStorm). */
const selectAllOccurrences: CmCommand = (view) => {
  const { state } = view;
  if (state.selection.ranges.every((r) => r.empty)) {
    const words = state.selection.ranges.map((r) => state.wordAt(r.head) ?? r);
    if (words.every((r) => r.empty)) return false;
    view.dispatch({ selection: EditorSelection.create(words, state.selection.mainIndex) });
  }
  return selectSelectionMatches(view);
};

const scrollBy = (lines: number): CmCommand => (view) => {
  view.scrollDOM.scrollTop += lines * view.defaultLineHeight;
  return true;
};

/** PHPStorm action id → CodeMirror command. */
export const EDITOR_COMMANDS: Record<string, CmCommand> = {
  EditorDuplicate: duplicate,
  EditorDeleteLine: deleteLine,
  MoveLineUp: moveLineUp,
  MoveLineDown: moveLineDown,
  // Statement-aware moving needs a language model; line moving is the closest fallback.
  MoveStatementUp: moveLineUp,
  MoveStatementDown: moveLineDown,
  // Line comments where the language has them, else block comments per line (CSS, Smarty), as in PHPStorm.
  CommentByLineComment: toggleComment,
  CommentByBlockComment: toggleBlockComment,
  EditorSelectWord: extendSelection,
  EditorUnSelectWord: shrinkSelection,
  SelectNextOccurrence: selectNextOccurrence,
  UnselectPreviousOccurrence: unselectOccurrence,
  SelectAllOccurrences: selectAllOccurrences,
  EditorJoinLines: joinLines,
  EditorToggleCase: toggleCase,
  EditorSplitLine: splitLine,
  EditorStartNewLine: insertBlankLine,
  EditorStartNewLineBefore: startNewLineBefore,
  AutoIndentLines: indentSelection,
  EditorMatchBrace: cursorMatchingBracket,
  ExpandRegion: unfoldCode,
  CollapseRegion: foldCode,
  ExpandRegionRecursively: unfoldCode,
  CollapseRegionRecursively: foldCode,
  ExpandAllRegions: unfoldAll,
  CollapseAllRegions: foldAll,
  CodeCompletion: startCompletion,
  // The Find/Replace bar opens collapsed for Find and with the replace row for Replace.
  Find: openFind,
  Replace: openReplace,
  // After Highlight Usages in File (Ctrl+Shift+F7), F3 / Shift+F3 jump between the usages.
  FindNext: (view) => gotoHighlightedUsage(view, 1) || findNext(view),
  FindPrevious: (view) => gotoHighlightedUsage(view, -1) || findPrevious(view),
  GotoLine: gotoLine,
  EditorScrollUp: scrollBy(-1),
  EditorScrollDown: scrollBy(1),
};

/** Run a CodeMirror command on the active editor and give it focus. */
export function runInEditor(cmd: CmCommand): void {
  const view = activeView();
  if (!view) return;
  cmd(view);
  // Commands run from the Find/Replace bar (F3, Ctrl+F) keep the focus in the bar.
  if (!view.dom.querySelector(".cm-find-bar")?.contains(document.activeElement)) view.focus();
}
