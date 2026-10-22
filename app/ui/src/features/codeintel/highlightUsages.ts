/**
 * Highlighting usages in the editor, from Rust (`code_highlight_usages`: exit points,
 * implementing methods, the language servers' `documentHighlight`, the PHP index, else a
 * lexical fallback):
 * - automatically for the element at the caret once the caret rests (Settings | Editor |
 *   General | "Highlight usages of element at caret");
 * - Highlight Usages in File (Ctrl+Shift+F7): sticky until Escape, F3 / Shift+F3 jump
 *   between the usages, the status bar says how many there are.
 */
import { ViewPlugin, type EditorView, type ViewUpdate } from "@codemirror/view";
import { codeApi, isLibraryPath, type HighlightAnswer } from "@/ipc/codeintel";
import { getSetting } from "@/state/settings";
import { showStatusMessage } from "@/features/statusItems";
import { showHint } from "@/features/lsp/hint";
import { syncNow } from "@/features/lsp/sync";
import { fromLsp, toLsp } from "@/features/lsp/util";
import {
  hasStickyUsages,
  onUsageHighlightsCleared,
  setUsageHighlights,
  usageHighlights,
  type UsageMark,
} from "@/editor/usageHighlight";

/** How long the caret rests before the element at it is highlighted. */
const CARET_DELAY_MS = 300;

export const ESCAPE_HINT = "(Press Escape to remove the highlighting)";

function marksOf(view: EditorView, answer: HighlightAnswer): UsageMark[] {
  const doc = view.state.doc;
  return answer.highlights.map((h) => ({ from: fromLsp(doc, h.range.start), to: fromLsp(doc, h.range.end), kind: h.kind }));
}

async function ask(path: string, view: EditorView, pos: number): Promise<HighlightAnswer | null> {
  await syncNow(path);
  return codeApi.highlightUsages(path, toLsp(view.state.doc, pos), view.state.doc.toString()).catch(() => null);
}

/** Highlight Usages in File (Ctrl+Shift+F7) at `pos`; again on the same usages removes them. */
export async function highlightUsagesInFile(path: string, view: EditorView, pos: number): Promise<void> {
  if (isLibraryPath(path)) return;
  const doc = view.state.doc;
  const answer = await ask(path, view, pos);
  if (view.state.doc !== doc) return;
  if (!answer?.highlights.length) {
    showHint(view, pos, answer?.summary ?? "No usages found in file");
    return;
  }
  const marks = marksOf(view, answer);
  const current = usageHighlights(view.state);
  const same =
    current?.sticky &&
    current.marks.length === marks.length &&
    current.marks.every((m, i) => m.from === marks[i].from && m.to === marks[i].to);
  if (same) {
    view.dispatch({ effects: setUsageHighlights.of(null) });
    showStatusMessage(null);
    return;
  }
  view.dispatch({ effects: setUsageHighlights.of({ marks, sticky: true, summary: answer.summary }) });
  showStatusMessage(`${answer.summary} ${ESCAPE_HINT}`);
}

onUsageHighlightsCleared(() => showStatusMessage(null));

/** The automatic highlighting of the element at the caret. */
export function caretUsageHighlighter(path: string) {
  return ViewPlugin.fromClass(
    class {
      timer: ReturnType<typeof setTimeout> | undefined;
      seq = 0;

      constructor(readonly view: EditorView) {
        this.schedule();
      }

      update(u: ViewUpdate) {
        if (!u.selectionSet && !u.docChanged && !u.focusChanged) return;
        if (u.selectionSet || u.docChanged) this.schedule();
      }

      schedule() {
        clearTimeout(this.timer);
        this.seq++;
        const current = usageHighlights(this.view.state);
        // Leaving the highlighted element removes its highlight right away.
        if (current && !current.sticky) {
          const head = this.view.state.selection.main.head;
          if (!current.marks.some((m) => m.from <= head && head <= m.to)) {
            queueMicrotask(() => {
              const now = usageHighlights(this.view.state);
              if (now && !now.sticky) this.view.dispatch({ effects: setUsageHighlights.of(null) });
            });
          }
        }
        this.timer = setTimeout(() => void this.run(), CARET_DELAY_MS);
      }

      async run() {
        const view = this.view;
        if (!getSetting<boolean>("editor.highlightUsagesAtCaret", true) || hasStickyUsages(view.state)) return;
        const sel = view.state.selection.main;
        if (!sel.empty || !view.hasFocus) return;
        const seq = this.seq;
        const doc = view.state.doc;
        const answer = await ask(path, view, sel.head);
        if (seq !== this.seq || view.state.doc !== doc || hasStickyUsages(view.state)) return;
        const marks = answer ? marksOf(view, answer) : [];
        const current = usageHighlights(view.state);
        if (!marks.length) {
          if (current) view.dispatch({ effects: setUsageHighlights.of(null) });
          return;
        }
        view.dispatch({ effects: setUsageHighlights.of({ marks, sticky: false, summary: answer!.summary }) });
      }

      destroy() {
        clearTimeout(this.timer);
        this.seq++;
      }
    },
  );
}
