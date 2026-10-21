/**
 * Code navigation like PHPStorm on top of Rust's code intelligence (`code_*` commands:
 * language servers first, the PHP index and the PHP stubs as fallback, through the
 * project's backend so remote projects work): Find Usages (Alt+F7), Show Usages
 * (Ctrl+Alt+F7), Go to Implementation (Ctrl+Alt+B), Type Declaration (Ctrl+Shift+B), Super
 * Method (Ctrl+U), the Find tool window (Alt+3), implemented / overridden gutter icons and
 * read-only library files. Go to Declaration (Ctrl+B, Ctrl+Click) lives in
 * `features/lsp/goto.ts` and shows usages on a declaration through this feature.
 */
import { createRoot, Show } from "solid-js";
import { render } from "solid-js/web";
import { Search } from "lucide-solid";
import type { EditorView } from "@codemirror/view";
import { registerToolWindow } from "@/features/registry";
import { registerCommand } from "@/state/commands";
import { actionById, showBalloon } from "@/keymap";
import { codeApi, type NavKind } from "@/ipc/codeintel";
import { activeView } from "@/features/lsp/navigation";
import { showHint } from "@/features/lsp/hint";
import { syncNow } from "@/features/lsp/sync";
import { toLsp } from "@/features/lsp/util";
import { setImplementationNavigator } from "@/features/lsp/goto";
import { isDialogWindow } from "@/shell/window/mode";
import { FindTabs, FindToolWindow, findGroupByType } from "./FindToolWindow";
import { ShowUsagesPopup } from "./ShowUsagesPopup";
import { chooseTargetAt } from "./targets";
import { findTabs, findUsagesAt, gotoFindOccurrence, showUsagesAt, usagesPopup } from "./usages";
import "./library";
import "./markers";
import "./codeintel.css";

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
// Ctrl+Alt+Down / Up: the next or previous result of the Find tool window's active tab.
action("NextOccurence", "Next Occurrence", "Navigate", () => {
  if (!gotoFindOccurrence(1, findGroupByType())) showBalloon("There are no occurrences", "Search with Find Usages (Alt+F7) or Find in Files first.");
});
action("PreviousOccurence", "Previous Occurrence", "Navigate", () => {
  if (!gotoFindOccurrence(-1, findGroupByType())) showBalloon("There are no occurrences", "Search with Find Usages (Alt+F7) or Find in Files first.");
});

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
  when: () => findTabs().length > 0,
});

if (typeof document !== "undefined" && !isDialogWindow()) {
  const host = document.createElement("div");
  host.id = "usages-overlay";
  document.body.appendChild(host);
  createRoot(() =>
    render(
      () => (
        <Show when={usagesPopup()} keyed>
          {(p) => <ShowUsagesPopup popup={p} />}
        </Show>
      ),
      host,
    ),
  );
}
