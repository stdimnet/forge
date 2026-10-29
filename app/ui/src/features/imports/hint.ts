/**
 * Auto Import's import popup ("Show import popup"): after the caret rests on or next to an
 * unresolved class name, a hint "? App\Models\User Alt+Enter" (or "? Multiple choices…")
 * appears above it, like PHPStorm. Alt+Enter while it shows imports the class right away
 * (with the "Class to Import" chooser when several classes match); JavaScript and
 * TypeScript hints come from the TypeScript server's "Add import from" fixes and Alt+Enter
 * opens the usual popup with them. Rust finds the names and candidates; this module only
 * shows the hint.
 */
import { StateEffect, StateField, type Extension } from "@codemirror/state";
import { EditorView, ViewPlugin, showTooltip, type Tooltip, type ViewUpdate } from "@codemirror/view";
import { completionStatus } from "@codemirror/autocomplete";
import { codeApi } from "@/ipc/codeintel";
import { importsApi, type ImportChoice } from "@/ipc/imports";
import { shortcutLabel } from "@/keymap";
import { getSetting } from "@/state/settings";
import { diagnosticsAt } from "@/features/lsp/diagnostics";
import { syncNow } from "@/features/lsp/sync";
import { h, toLsp } from "@/features/lsp/util";
import { intentionPopup } from "@/features/codeintel/intentions";
import { chooseClassToImport, importClass } from "./chooser";
import { hintText, isPhpFile, isWebScript, webImportModule } from "./model";

/** PHPStorm shows the popup shortly after typing stops. */
export const HINT_DELAY_MS = 600;

interface ImportHint {
  /** Where the name is (document offsets). */
  start: number;
  end: number;
  text: string;
  /** PHP: what Alt+Enter imports. */
  choice?: ImportChoice;
}

const setImportHint = StateEffect.define<ImportHint | null>();

const importHintField = StateField.define<ImportHint | null>({
  create: () => null,
  update(hint, tr) {
    for (const e of tr.effects) if (e.is(setImportHint)) return e.value;
    if (!hint || tr.docChanged) return null;
    // Moving the caret away from the line hides it.
    if (tr.selection) {
      const head = tr.state.selection.main.head;
      const line = tr.state.doc.lineAt(hint.start);
      if (head < line.from || head > line.to) return null;
    }
    return hint;
  },
  provide: (f) =>
    showTooltip.from(f, (hint): Tooltip | null =>
      hint
        ? {
            pos: hint.start,
            above: true,
            create: () => ({ dom: h("div", "cm-import-hint", hint.text) }),
          }
        : null,
    ),
});

/** The import popup shown in `view`, if any. */
export function importHintOf(view: EditorView): ImportHint | null {
  return view.state.field(importHintField, false) ?? null;
}

/**
 * Alt+Enter while the popup shows: import its class (PHP). False when there is no PHP
 * popup, so the usual context actions open.
 */
export function takeImportHint(path: string, view: EditorView): boolean {
  const hint = importHintOf(view);
  if (!hint?.choice) return false;
  view.dispatch({ effects: setImportHint.of(null) });
  const choice = hint.choice;
  if (choice.candidates.length === 1) void importClass(view, choice, choice.candidates[0]);
  else chooseClassToImport(path, view, choice);
  return true;
}

class HintPlugin {
  timer: ReturnType<typeof setTimeout> | undefined;
  seq = 0;

  constructor(
    readonly view: EditorView,
    readonly path: string,
  ) {}

  update(u: ViewUpdate) {
    if (u.docChanged || u.selectionSet || u.focusChanged) this.schedule();
  }

  schedule() {
    clearTimeout(this.timer);
    const id = ++this.seq;
    this.timer = setTimeout(() => void this.query(id), HINT_DELAY_MS);
  }

  async query(id: number) {
    const view = this.view;
    if (!view.hasFocus || intentionPopup() || completionStatus(view.state) || !view.state.selection.main.empty) return;
    const doc = view.state.doc;
    const caret = view.state.selection.main.head;
    const hint = isPhpFile(this.path) ? await this.phpHint(caret) : await this.webHint(caret);
    if (id !== this.seq || view.state.doc !== doc || view.state.selection.main.head !== caret) return;
    if (hint || importHintOf(view)) view.dispatch({ effects: setImportHint.of(hint) });
  }

  async phpHint(caret: number): Promise<ImportHint | null> {
    if (!getSetting<boolean>("php.autoImport.showPopup", true)) return null;
    const found = await importsApi.phpHint(this.path, this.view.state.doc.toString(), caret).catch(() => null);
    if (!found || !found.candidates.length) return null;
    return {
      start: found.start,
      end: found.end,
      text: hintText(found.candidates, shortcutLabel("ShowIntentionActions")),
      choice: { start: found.start, end: found.end, name: found.name, doc: found.doc, candidates: found.candidates },
    };
  }

  async webHint(caret: number): Promise<ImportHint | null> {
    if (!getSetting<boolean>("javascript.autoImport.showPopup", true)) return null;
    // Only on an unresolved name: TypeScript reports "Cannot find name 'x'".
    const unresolved = diagnosticsAt(this.view.state, caret).some((d) => /cannot find name/i.test(d.message));
    if (!unresolved) return null;
    await syncNow(this.path);
    const doc = this.view.state.doc;
    const answer = await codeApi.contextActions(this.path, { start: toLsp(doc, caret), end: toLsp(doc, caret) }, doc.toString()).catch(() => null);
    const modules = (answer?.actions ?? []).map((a) => webImportModule(a.title)).filter((m): m is string => !!m);
    if (!modules.length) return null;
    const word = this.view.state.wordAt(caret);
    const start = word?.from ?? caret;
    const name = word ? doc.sliceString(word.from, word.to) : "";
    const what = modules.length === 1 ? [`${name} from "${modules[0]}"`] : modules;
    return { start, end: word?.to ?? caret, text: hintText(what, shortcutLabel("ShowIntentionActions")) };
  }

  destroy() {
    clearTimeout(this.timer);
  }
}

/** Editor extension: the import popup of PHP, JavaScript, TypeScript and Vue files. */
export function importHint(path: string): Extension {
  if (!isPhpFile(path) && !isWebScript(path)) return [];
  return [importHintField, ViewPlugin.define((view) => new HintPlugin(view, path))];
}
