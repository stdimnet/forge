/**
 * Navigate | Test / Test Subject (`GotoTest`, Ctrl+Shift+T): from a class, component or
 * module to its tests, from a test to its subject. One target opens directly; several open
 * the chooser ("Choose Test for Order (2 found)") with "Create New Test..." at the top; none
 * open it with only "Create New Test...", which opens the Create New PHP Test / Create New
 * Test dialog. Computed in Rust next to the files (`code_test_targets`,
 * `code_new_test_plan`, `forge_codeintel::test_nav`), so remote projects work too.
 */
import { render } from "solid-js/web";
import { registerCommand } from "@/state/commands";
import { activeTab } from "@/state/editor";
import { codeApi, type TestTargets } from "@/ipc/codeintel";
import { activeView } from "@/features/lsp/navigation";
import { showHint } from "@/features/lsp/hint";
import { chooseDeclaration, chooseDeclarationAt, type ChooserAction } from "@/features/navigation/declarationChooser";
import { relatedTarget } from "@/features/related";
import { showBalloon } from "@/keymap";
import { isDialogWindow } from "@/shell/window/mode";
import { CreateTestDialogHost } from "./CreateTestDialog";
import { chooserTitle, CREATE_NEW_TEST, nothingFound, setCreateTestPlan } from "./model";
import { t as tr } from "@/i18n";

const warn = (what: string) => (e: unknown) => {
  console.warn(what, e);
  return null;
};

/** The editor's current file and text (the view's text when it has unsaved changes). */
function current() {
  const tab = activeTab();
  if (!tab) return null;
  const a = activeView();
  const view = a?.path === tab.path ? a.view : undefined;
  return { path: tab.path, view, content: view?.state.doc.toString() ?? tab.content };
}

/** Open the Create New Test dialog for the current file. */
export async function createNewTest(): Promise<void> {
  const cur = current();
  if (!cur) return;
  const plan = await codeApi.newTestPlan(cur.path, cur.content).catch(warn("create test failed"));
  if (plan) setCreateTestPlan(plan);
  else showBalloon(tr("tools.goToTest.testsCannotBeCreatedFor"));
}

async function gotoTest(): Promise<void> {
  const cur = current();
  if (!cur) return;
  const found: TestTargets | null = await codeApi.testTargets(cur.path, cur.content).catch(warn("go to test failed"));
  if (!found) return;
  const targets = found.targets.map((t) => ({ ...relatedTarget(t), group: undefined }));
  const actions: ChooserAction[] = found.canCreate ? [{ label: CREATE_NEW_TEST, run: () => void createNewTest() }] : [];
  if (!targets.length && !actions.length) {
    const text = nothingFound(found);
    if (cur.view) showHint(cur.view, cur.view.state.selection.main.head, text);
    else showBalloon(text);
    return;
  }
  const opts = { title: chooserTitle(found), actions };
  if (cur.view) chooseDeclarationAt(cur.view, cur.view.state.selection.main.head, targets, opts);
  else chooseDeclaration(targets, { x: window.innerWidth / 2 - 200, y: window.innerHeight / 4, ...opts });
}

registerCommand({ id: "GotoTest", title: "Test", category: "Navigate", run: () => void gotoTest() });

if (typeof document !== "undefined" && !isDialogWindow()) {
  const host = document.createElement("div");
  host.id = "create-test-overlay";
  document.body.appendChild(host);
  render(CreateTestDialogHost, host);
}
