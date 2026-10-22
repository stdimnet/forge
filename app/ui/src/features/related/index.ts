/**
 * Navigate | Related Symbol... (Ctrl+Alt+Home): the files that belong to the current one —
 * a class's tests (and back), a Laravel controller's views and routes, a view's controllers,
 * a model's migrations, factory and policy, a Symfony controller's Twig templates, a
 * component's tests, stories and styles. Computed in Rust next to the files
 * (`code_related`, `forge_codeintel::related`); one result opens directly, several open the
 * shared chooser grouped by kind.
 */
import { registerCommand } from "@/state/commands";
import { activeTab } from "@/state/editor";
import { codeApi, type RelatedItem } from "@/ipc/codeintel";
import { activeView } from "@/features/lsp/navigation";
import { showHint } from "@/features/lsp/hint";
import { chooseDeclaration, chooseDeclarationAt, type DeclarationTarget } from "@/features/navigation/declarationChooser";
import { showBalloon } from "@/keymap";

const dirName = (path: string) => path.split("/").slice(0, -1).join("/");

/** Items that are files show their folder after the name; routes and actions their file. */
export function relatedTarget(item: RelatedItem): DeclarationTarget {
  const isFile = item.line === 0 && item.name === item.path.split("/").pop();
  return {
    path: item.path,
    line: item.line,
    column: item.column,
    name: item.name,
    container: isFile ? dirName(item.path) || null : null,
    group: item.group,
    wholeFile: isFile || undefined,
  };
}

registerCommand({
  id: "GotoRelated",
  title: "Related Symbol...",
  category: "Navigate",
  run: async () => {
    const tab = activeTab();
    if (!tab) return;
    const a = activeView();
    const view = a?.path === tab.path ? a.view : undefined;
    const content = view?.state.doc.toString() ?? tab.content;
    const items = await codeApi.related(tab.path, content).catch((e) => {
      console.warn("related symbols failed", e);
      return [] as RelatedItem[];
    });
    const targets = items.map(relatedTarget);
    const title = "Choose Target";
    if (!targets.length) {
      if (view) showHint(view, view.state.selection.main.head, "No related symbols");
      else showBalloon("No related symbols");
      return;
    }
    if (view) chooseDeclarationAt(view, view.state.selection.main.head, targets, { title });
    else chooseDeclaration(targets, { x: window.innerWidth / 2 - 200, y: window.innerHeight / 4, title });
  },
});
