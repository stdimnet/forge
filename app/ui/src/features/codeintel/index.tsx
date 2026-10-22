/**
 * Code navigation like PHPStorm on top of Rust's code intelligence (`code_*` commands:
 * language servers first, the PHP index and the PHP stubs as fallback, through the
 * project's backend so remote projects work): Find Usages (Alt+F7), Show Usages
 * (Ctrl+Alt+F7), Find Usages in File (Ctrl+F7), Highlight Usages in File (Ctrl+Shift+F7)
 * and the highlighting of the element at the caret, the Find Usages dialog
 * (Ctrl+Alt+Shift+F7), Go to Implementation (Ctrl+Alt+B), Type Declaration (Ctrl+Shift+B), Super
 * Method (Ctrl+U), the Find tool window (Alt+3), implemented / overridden gutter icons,
 * read-only library files, Rename (Shift+F6, in place), Extract / Introduce and Inline
 * (`extract.ts`), Extract Interface / Superclass and Pull Members Up / Push Members Down
 * (`hierarchyRefactorings.ts`), Refactor This (`refactorThis.ts`) and Show Context Actions (Alt+Enter, with the light bulb). Go to Declaration (Ctrl+B, Ctrl+Click) lives in
 * `features/lsp/goto.ts` and shows usages on a declaration through this feature.
 */
import { createRoot, Show } from "solid-js";
import { render } from "solid-js/web";
import { Search } from "lucide-solid";
import type { EditorView } from "@codemirror/view";
import { registerToolWindow } from "@/features/registry";
import { registerEditorExtension } from "@/editor/extensions";
import { isLibraryPath } from "@/ipc/codeintel";
import { renameEntryAt, renameSelection } from "@/features/project/fileMenu";
import { projectTree } from "@/features/project/ProjectView";
import { activeTab } from "@/state/editor";
import { registerCommand } from "@/state/commands";
import { actionById, isActionAvailable, runAction, shortcutLabel, showBalloon } from "@/keymap";
import { codeApi, type NavKind } from "@/ipc/codeintel";
import { activeView } from "@/features/lsp/navigation";
import { showHint } from "@/features/lsp/hint";
import { syncNow } from "@/features/lsp/sync";
import { toLsp } from "@/features/lsp/util";
import { setImplementationNavigator } from "@/features/lsp/goto";
import { isDialogWindow } from "@/shell/window/mode";
import { registerMainMenuEntries } from "@/features/mainMenu";
import { action as menuAction } from "@/features/mainMenu/model";
import { FindTabs, FindToolWindow, findGroupByType } from "./FindToolWindow";
import { ShowUsagesPopup } from "./ShowUsagesPopup";
import { IntentionPopup } from "./IntentionPopup";
import { intentionBulb, intentionPopup, showContextActions } from "./intentions";
import { RefactoringPreviewDialog, RenameDialog } from "./RefactoringDialogs";
import { ChooserPopup, ExtractMethodDialog, InlineDialog, IntroduceDialog } from "./ExtractDialogs";
import {
  chooser,
  extractMethod,
  extractMethodDialog,
  inline,
  inlineDialog,
  introduce,
  introduceDialog,
  refactorHighlight,
} from "./extract";
import { refactorThisAt, refactorThisPopup } from "./refactorThis";
import { RefactorThisPopup } from "./RefactorThisPopup";
import { extractInterfaceAt, extractSuperclassAt, pullUpAt, pushDownAt } from "./hierarchyRefactorings";
import { HierarchyDialogHost } from "./HierarchyDialogs";
import { listenForServerEdits, refactoringHistory } from "./refactoring";
import { MoveDialogHost } from "./MoveDialogs";
import {
  changeSignatureAt,
  copyAt,
  copyEntries,
  makeStaticAt,
  moveAt,
  moveEntries,
  safeDeleteAt,
  safeDeleteEntries,
} from "./moveRefactorings";
import { inPlaceRename, previewRequest, renameDialog, startRename } from "./rename";
import { chooseTargetAt } from "./targets";
import { findTabs, findUsagesAt, findUsagesInFileAt, searchTabOpen, gotoFindOccurrence, showUsagesAt, usagesPopup } from "./usages";
import { findUsagesDialog, openFindUsagesSettings } from "./findSettings";
import { FindUsagesDialog } from "./FindUsagesDialog";
import { caretUsageHighlighter, highlightUsagesInFile } from "./highlightUsages";
import "./library";
import "./markers";
import "./codeintel.css";
import "./refactoring.css";

const NAV_TITLES: Record<NavKind, { title: string; none: string }> = {
  implementation: { title: "Choose Implementation", none: "No implementations found" },
  superMethod: { title: "Choose Super Method", none: "No super method found" },
  typeDeclaration: { title: "Choose Type Declaration", none: "Cannot find type declaration to go to" },
};

/** Go to Implementation / Super Method / Type Declaration at `pos`. */
export async function navigateAt(kind: NavKind, path: string, view: EditorView, pos: number): Promise<void> {
  await syncNow(path);
  const answer = await codeApi
    .navigate(path, toLsp(view.state.doc, pos), kind, view.state.doc.toString())
    .catch((e) => ({ targets: [], missing: { feature: kind, message: String(e instanceof Error ? e.message : e), premium: false } }));
  if (!answer.targets.length) {
    // Intelephense without a licence has no implementations; say so instead of nothing.
    showHint(view, pos, answer.missing?.message ?? NAV_TITLES[kind].none);
    return;
  }
  chooseTargetAt(view, pos, answer.targets, `${NAV_TITLES[kind].title} (${answer.targets.length} found)`);
}

setImplementationNavigator((path, view, pos) => navigateAt("implementation", path, view, pos));

const inEditor = (fn: (path: string, view: EditorView, pos: number) => unknown) => () => {
  const a = activeView();
  if (a) void fn(a.path, a.view, a.view.state.selection.main.head);
};

function action(id: string, fallbackTitle: string, category: string, run: () => void) {
  registerCommand({ id, title: actionById(id)?.title ?? fallbackTitle, category, run });
}

action("GotoImplementation", "Implementation(s)", "Navigate", inEditor((p, v, pos) => navigateAt("implementation", p, v, pos)));
action("GotoTypeDeclaration", "Type Declaration", "Navigate", inEditor((p, v, pos) => navigateAt("typeDeclaration", p, v, pos)));
action("GotoSuperMethod", "Super Method", "Navigate", inEditor((p, v, pos) => navigateAt("superMethod", p, v, pos)));
action("FindUsages", "Find Usages", "Find", inEditor(findUsagesAt));
action("ShowUsages", "Show Usages", "Find", inEditor((p, v, pos) => showUsagesAt(p, v, pos)));
action("FindUsagesInFile", "Find Usages in File", "Find", inEditor(findUsagesInFileAt));
action("HighlightUsagesInFile", "Highlight Usages in File", "Find", inEditor(highlightUsagesInFile));
// Ctrl+Alt+Shift+F7: the Find Usages dialog, then Find Usages with the chosen settings.
action(
  "ShowSettingsAndFindUsages",
  "Find Usages Settings…",
  "Find",
  inEditor((p, v, pos) => openFindUsagesSettings(p, v, pos, () => void findUsagesAt(p, v, pos))),
);
// Ctrl+Alt+Down / Up: the next or previous result of the Find tool window's active tab.
action("NextOccurence", "Next Occurrence", "Navigate", () => {
  if (!gotoFindOccurrence(1, findGroupByType())) showBalloon("There are no occurrences", "Search with Find Usages (Alt+F7) or Find in Files first.");
});
action("PreviousOccurence", "Previous Occurrence", "Navigate", () => {
  if (!gotoFindOccurrence(-1, findGroupByType())) showBalloon("There are no occurrences", "Search with Find Usages (Alt+F7) or Find in Files first.");
});
registerMainMenuEntries("Navigate", [menuAction("NextOccurence"), menuAction("PreviousOccurence")], {
  after: "GotoPreviousError",
});

action("ShowIntentionActions", "Show Context Actions", "Code", inEditor((p, v) => showContextActions(p, v)));

/**
 * Shift+F6: the Project view renames the selected file or folder; the editor renames the
 * symbol at the caret in place (again during the in-place rename: the Rename dialog).
 */
action("RenameElement", "Rename…", "Refactor", () => {
  const focused = document.activeElement;
  if (focused?.closest(".project-tree")) {
    void renameSelection();
    return;
  }
  if (focused?.closest(".dialog")) return;
  const a = activeView();
  if (a) void startRename(a.path, a.view);
});
action("RenameFile", "Rename File…", "Refactor", () => {
  const tab = activeTab();
  if (tab && !isLibraryPath(tab.path)) void renameEntryAt(tab.path);
});

// Extract / Introduce (Ctrl+Alt+V / C / F / P, Ctrl+Alt+M) and Inline (Ctrl+Alt+N). In the
// editor these win over global bindings of the same keys (the Vim toggle on Ctrl+Alt+V).
action("IntroduceVariable", "Introduce Variable…", "Refactor", inEditor((p, v) => introduce("variable", p, v)));
action("IntroduceConstant", "Introduce Constant…", "Refactor", inEditor((p, v) => introduce("constant", p, v)));
action("IntroduceField", "Introduce Field…", "Refactor", inEditor((p, v) => introduce("field", p, v)));
action("IntroduceParameter", "Introduce Parameter…", "Refactor", inEditor((p, v) => introduce("parameter", p, v)));
action("ExtractMethod", "Extract Method…", "Refactor", inEditor((p, v) => extractMethod(p, v)));
action("Inline", "Inline…", "Refactor", inEditor((p, v) => inline(p, v)));
// Move (F6), Copy (F5) and Safe Delete (Alt+Delete): the selected file or folder in the
// Project view, else the element at the caret. Change Signature (Ctrl+F6), Make Static.
function treeOrEditor(entries: (paths: string[]) => unknown, element: (path: string, view: EditorView) => unknown) {
  return () => {
    const focused = document.activeElement;
    if (focused?.closest(".project-tree")) {
      const selected = projectTree.selectedPaths().filter((p) => !p.includes("://"));
      if (selected.length) void entries(selected);
      return;
    }
    if (focused?.closest(".dialog")) return;
    const a = activeView();
    if (a) void element(a.path, a.view);
  };
}
action("Move", "Move…", "Refactor", treeOrEditor(moveEntries, moveAt));
action("CopyElement", "Copy…", "Refactor", treeOrEditor(copyEntries, copyAt));
action("SafeDelete", "Safe Delete…", "Refactor", treeOrEditor(safeDeleteEntries, safeDeleteAt));
action("ChangeSignature", "Change Signature…", "Refactor", inEditor((p, v) => changeSignatureAt(p, v)));
action("MakeStatic", "Make Static…", "Refactor", inEditor((p, v) => makeStaticAt(p, v)));
// Extract Interface / Superclass, Pull Members Up / Push Members Down (PHP classes).
action("ExtractInterface", "Extract Interface…", "Refactor", inEditor((p, v) => extractInterfaceAt(p, v)));
action("ExtractSuperclass", "Extract Superclass…", "Refactor", inEditor((p, v) => extractSuperclassAt(p, v)));
action("MembersPullUp", "Pull Members Up…", "Refactor", inEditor((p, v) => pullUpAt(p, v)));
action("MemberPushDown", "Push Members Down…", "Refactor", inEditor((p, v) => pushDownAt(p, v)));

// Ctrl+Alt+Shift+T: the refactorings that apply at the caret, in a popup.
action(
  "Refactorings.QuickListPopupAction",
  "Refactor This…",
  "Refactor",
  inEditor((p, v) =>
    refactorThisAt(p, v, {
      available: isActionAvailable,
      title: (id) => actionById(id)?.title ?? id,
      shortcut: shortcutLabel,
      run: (id) => void runAction(id),
    }),
  ),
);

// Rename, the light bulb and refactoring undo in every project file editor.
registerEditorExtension(({ path }) =>
  isLibraryPath(path)
    ? []
    : [refactoringHistory(), inPlaceRename(), intentionBulb(path), caretUsageHighlighter(path), refactorHighlight()],
);

registerToolWindow({
  id: "find",
  title: "Find",
  icon: Search,
  anchor: "bottom",
  order: 5,
  shortcut: "Alt+3",
  component: FindToolWindow,
  headerContent: FindTabs,
  // Like PHPStorm, the stripe button appears once there are results.
  when: () => findTabs().length > 0 || searchTabOpen(),
});

if (typeof document !== "undefined" && !isDialogWindow()) {
  listenForServerEdits();
  const host = document.createElement("div");
  host.id = "usages-overlay";
  document.body.appendChild(host);
  createRoot(() =>
    render(
      () => (
        <>
          <Show when={usagesPopup()} keyed>
            {(p) => <ShowUsagesPopup popup={p} />}
          </Show>
          <Show when={intentionPopup()} keyed>
            {(p) => <IntentionPopup popup={p} />}
          </Show>
          <Show when={renameDialog()} keyed>
            {(r) => <RenameDialog req={r} />}
          </Show>
          <Show when={previewRequest()} keyed>
            {(r) => <RefactoringPreviewDialog req={r} />}
          </Show>
          <Show when={findUsagesDialog()} keyed>
            {(r) => <FindUsagesDialog req={r} />}
          </Show>
          <Show when={chooser()} keyed>
            {(r) => <ChooserPopup req={r} />}
          </Show>
          <Show when={introduceDialog()} keyed>
            {(r) => <IntroduceDialog req={r} />}
          </Show>
          <Show when={extractMethodDialog()} keyed>
            {(r) => <ExtractMethodDialog req={r} />}
          </Show>
          <Show when={inlineDialog()} keyed>
            {(r) => <InlineDialog req={r} />}
          </Show>
          <MoveDialogHost />
          <HierarchyDialogHost />
          <Show when={refactorThisPopup()} keyed>
            {(r) => <RefactorThisPopup req={r} />}
          </Show>
        </>
      ),
      host,
    ),
  );
}
