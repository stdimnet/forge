/**
 * Auto Import on the fly: "Add unambiguous imports on the fly" and "Optimize imports on the
 * fly" (Settings | Editor | General | Auto Import). After the user paused typing, Rust
 * decides what to add or remove (PHP: `phpforge_lang_php::imports::auto::on_the_fly`;
 * JavaScript / TypeScript: the TypeScript server's source actions, checked by
 * `forge_codeintel::imports::on_the_fly_allowed`) and never touches the caret's line, the
 * text edited last or the `use` block being edited. The editor only tracks the last edit
 * and applies the answer.
 */
import type { Extension } from "@codemirror/state";
import { ViewPlugin, type EditorView, type ViewUpdate } from "@codemirror/view";
import { completionStatus } from "@codemirror/autocomplete";
import { importsApi } from "@/ipc/imports";
import { getSetting } from "@/state/settings";
import { syncNow } from "@/features/lsp/sync";
import { toLsp } from "@/features/lsp/util";
import { intentionPopup } from "@/features/codeintel/intentions";
import { editsOf, isPhpFile, isWebScript, lspChanges, offsetChanges } from "./model";

/** How long typing must pause before imports change. */
export const ON_THE_FLY_DELAY_MS = 1500;

/** The on-the-fly settings of a file's language. */
export function onTheFlySettings(path: string): { add: boolean; optimize: boolean } {
  const prefix = isPhpFile(path) ? "php" : "javascript";
  return {
    add: getSetting<boolean>(`${prefix}.autoImport.addOnTheFly`, false),
    optimize: getSetting<boolean>(`${prefix}.autoImport.optimizeOnTheFly`, false),
  };
}

class OnTheFlyPlugin {
  timer: ReturnType<typeof setTimeout> | undefined;
  /** The range changed by the user's edits since the last check (current document offsets). */
  edited: { from: number; to: number } | null = null;
  seq = 0;

  constructor(
    readonly view: EditorView,
    readonly path: string,
  ) {}

  update(u: ViewUpdate) {
    if (!u.docChanged) return;
    // Our own changes are no user edits.
    if (u.transactions.some((t) => t.isUserEvent("input.autoImport"))) return;
    let edited = this.edited ? { from: u.changes.mapPos(this.edited.from, -1), to: u.changes.mapPos(this.edited.to, 1) } : null;
    u.changes.iterChangedRanges((_fa, _ta, fromB, toB) => {
      edited = edited ? { from: Math.min(edited.from, fromB), to: Math.max(edited.to, toB) } : { from: fromB, to: toB };
    });
    this.edited = edited;
    this.schedule();
  }

  schedule() {
    clearTimeout(this.timer);
    const { add, optimize } = onTheFlySettings(this.path);
    if (!add && !optimize) return;
    const id = ++this.seq;
    this.timer = setTimeout(() => void this.run(id), ON_THE_FLY_DELAY_MS);
  }

  async run(id: number) {
    const view = this.view;
    const { add, optimize } = onTheFlySettings(this.path);
    if ((!add && !optimize) || intentionPopup() || completionStatus(view.state)) return;
    const doc = view.state.doc;
    const caret = view.state.selection.main.head;
    const edited = this.edited;
    let changes: { from: number; to: number; insert: string }[] = [];
    if (isPhpFile(this.path)) {
      const answer = await importsApi
        .phpOnTheFly(this.path, doc.toString(), { caret, editedFrom: edited?.from ?? null, editedTo: edited?.to ?? null, add, optimize })
        .catch(() => null);
      if (!answer?.edits.length || id !== this.seq || view.state.doc !== doc) return;
      changes = offsetChanges(answer.edits);
    } else {
      await syncNow(this.path);
      if (id !== this.seq || view.state.doc !== doc) return;
      const range = edited ? { start: toLsp(doc, edited.from), end: toLsp(doc, edited.to) } : null;
      const answer = await importsApi
        .webOnTheFly({ path: this.path, content: doc.toString(), add, optimize, caret: toLsp(doc, caret), edited: range })
        .catch(() => null);
      if (!answer || id !== this.seq || view.state.doc !== doc) return;
      changes = lspChanges(view, editsOf(answer, this.path));
    }
    if (!changes.length) return;
    this.edited = null;
    view.dispatch({ changes, userEvent: "input.autoImport", scrollIntoView: false });
  }

  destroy() {
    clearTimeout(this.timer);
  }
}

/** Editor extension: Auto Import on the fly for PHP, JavaScript, TypeScript and Vue files. */
export function importsOnTheFly(path: string): Extension {
  if (!isPhpFile(path) && !isWebScript(path)) return [];
  return ViewPlugin.define((view) => new OnTheFlyPlugin(view, path));
}
