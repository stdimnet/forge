/**
 * Spelling and grammar like PHPStorm: the "Typo" (green squiggles) and "Grammar"
 * inspections in comments, strings, declared names, Markdown and plain text files and
 * commit messages, their Alt+Enter fixes ("Typo: Change to...", "Typo: Rename to...", "Save
 * 'word' to project-level / application-level dictionary", grammar replacements) and the
 * Settings | Editor | Natural Languages pages. Words, suggestions and rules come from Rust
 * (`forge-spell`); this feature collects the text and renders the answers.
 */
import { createEffect, createRoot, on } from "solid-js";
import type { EditorView } from "@codemirror/view";
import { registerEditorExtension } from "@/editor/extensions";
import { isLibraryPath, type ContextAction } from "@/ipc/codeintel";
import type { ChangeToData, ReplaceData, SaveToData } from "@/ipc/spell";
import { spellApi } from "@/ipc/spell";
import { registerSettingsPage } from "@/features/settingsPages";
import { SchemaPage } from "@/features/settings/SchemaPage";
import { showHint } from "@/features/lsp/hint";
import { registerContextActionHandler, registerContextActionProvider, showActionsPopup } from "@/features/codeintel/intentions";
import { commitRename, renameSessionOf, startRename } from "@/features/codeintel/rename";
import { settingsState } from "@/state/settings";
import { inspectorOf, invalidateAll, spellInspection } from "./inspector";
import { dictionaryWords, saveWord, settingsFingerprint } from "./options";
import { GRAMMAR_PAGE, NATURAL_LANGUAGES_PAGE, NaturalLanguagesPage, SPELLING_PAGE } from "./NaturalLanguagesPage";
import "./spelling.css";
import { t } from "@/i18n";

registerEditorExtension(({ path }) => (isLibraryPath(path) ? null : spellInspection(path)));

// Settings, dictionaries or the plugin's state changed: check open editors again.
createRoot(() => createEffect(on(settingsFingerprint, () => invalidateAll(), { defer: true })));

// ---------- Alt+Enter ----------

registerContextActionProvider(async (_path, view) => {
  const problems = inspectorOf(view)?.problemsAt(view.state.selection.main.head) ?? [];
  if (!problems.length) return [];
  return spellApi.contextActions({ problems, words: dictionaryWords(), hasProject: settingsState.hasProject });
});

const REPLACE = "SpellCheck.Replace";
const RENAME_APPLY = "SpellCheck.RenameApply";

const fix = (title: string, action: string, data: unknown): ContextAction => ({
  title,
  kind: "quickFix",
  source: "spelling",
  isPreferred: false,
  needsResolve: false,
  action,
  data,
});

/** Replace `from..to` with `text` as one undo step. */
export function replaceText(view: EditorView, { from, to, text }: ReplaceData): void {
  if (to > view.state.doc.length) return;
  view.dispatch({ changes: { from, to, insert: text }, selection: { anchor: from + text.length }, userEvent: "input.replace" });
  view.focus();
}

registerContextActionHandler(REPLACE, (data, { view }) => replaceText(view, data as ReplaceData));

// "Typo: Change to...": the suggestions in a second popup.
registerContextActionHandler("SpellCheck.ChangeTo", (data, { path, view }) => {
  const d = data as ChangeToData;
  if (view.state.sliceDoc(d.from, d.to) !== d.word) return;
  showActionsPopup(
    path,
    view,
    d.suggestions.map((s) => fix(s, REPLACE, { from: d.from, to: d.to, text: s } satisfies ReplaceData)),
  );
});

// "Typo: Rename to...": the suggested names, then the Rename refactoring with the chosen one.
registerContextActionHandler("SpellCheck.RenameTo", (data, { path, view }) => {
  const d = data as ChangeToData;
  showActionsPopup(
    path,
    view,
    d.suggestions.map((name) => fix(name, RENAME_APPLY, { from: d.from, to: d.to, text: name } satisfies ReplaceData)),
  );
});

registerContextActionHandler(RENAME_APPLY, async (data, { path, view }) => {
  const d = data as ReplaceData;
  view.dispatch({ selection: { anchor: d.from + 1 } });
  await startRename(path, view);
  if (renameSessionOf(view)) await commitRename(view, { name: d.text });
  else showHint(view, d.from, `Cannot rename to '${d.text}' here`);
});

registerContextActionHandler("SpellCheck.SaveTo", (data) => saveWord(data as SaveToData));

// ---------- Settings ----------

registerSettingsPage({
  id: NATURAL_LANGUAGES_PAGE,
  title: t("settings.page.natural-languages"),
  group: "Editor",
  order: 45,
  keywords: ["spelling", "grammar", "dictionary", "typo", "language"],
  component: NaturalLanguagesPage,
});
registerSettingsPage({
  id: SPELLING_PAGE,
  title: t("settings.page.spelling"),
  group: "Editor",
  parent: NATURAL_LANGUAGES_PAGE,
  order: 0,
  keywords: ["typo", "dictionary", "hunspell", "words", "spell checker"],
  component: () => <SchemaPage pageId={SPELLING_PAGE} />,
});
registerSettingsPage({
  id: GRAMMAR_PAGE,
  title: t("settings.page.grammar-and-style"),
  group: "Editor",
  parent: NATURAL_LANGUAGES_PAGE,
  order: 1,
  keywords: ["grammar", "style", "article", "repeated word"],
  component: () => <SchemaPage pageId={GRAMMAR_PAGE} />,
});
