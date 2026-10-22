/**
 * Hierarchies like PHPStorm (Navigate | Type Hierarchy, Method Hierarchy, Call Hierarchy):
 * the Hierarchy tool window on the right and the actions `TypeHierarchy` (Ctrl+H),
 * `MethodHierarchy` (Ctrl+Shift+H) and `CallHierarchy` (Ctrl+Alt+H), bound through the
 * keymap. Data from `code_*_hierarchy` (language servers, the PHP index), views from
 * `forge_codeintel::hierarchy` (`model.ts`).
 */
import { Network } from "lucide-solid";
import type { EditorView } from "@codemirror/view";
import { registerToolWindow } from "@/features/registry";
import { registerCommand } from "@/state/commands";
import { actionById, registerKeymapActions } from "@/keymap";
import { activeView } from "@/features/lsp/navigation";
import { HierarchyTabs, HierarchyToolWindow } from "./HierarchyToolWindow";
import { callHierarchyAt, HIERARCHY_WINDOW, methodHierarchyAt, typeHierarchyAt } from "./state";
import "./hierarchy.css";

registerToolWindow({
  id: HIERARCHY_WINDOW,
  title: "Hierarchy",
  icon: Network,
  anchor: "right",
  order: 20,
  component: HierarchyToolWindow,
  headerContent: HierarchyTabs,
});

// PHPStorm has no default shortcut for the window itself; it is listed under View | Tool Windows.
registerKeymapActions([{ id: "ActivateHierarchyToolWindow", title: "Hierarchy", group: "Main Menu/View/Tool Windows" }]);

const inEditor = (fn: (path: string, view: EditorView, pos: number) => Promise<void>) => () => {
  const a = activeView();
  if (a) void fn(a.path, a.view, a.view.state.selection.main.head);
};

function action(id: string, fallbackTitle: string, run: () => void) {
  registerCommand({ id, title: actionById(id)?.title ?? fallbackTitle, category: "Navigate", run });
}

action("TypeHierarchy", "Type Hierarchy", inEditor(typeHierarchyAt));
action("MethodHierarchy", "Method Hierarchy", inEditor(methodHierarchyAt));
action("CallHierarchy", "Call Hierarchy", inEditor(callHierarchyAt));
