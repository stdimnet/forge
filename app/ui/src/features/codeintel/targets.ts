/**
 * Navigation targets from Rust (`NavTarget`: Go to Declaration, Implementation, Super Method,
 * Type Declaration, gutter markers) into the shared Choose Declaration popup and the editor.
 */
import type { EditorView } from "@codemirror/view";
import { openFileAt } from "@/editor/views";
import type { NavTarget } from "@/ipc/codeintel";
import { chooseDeclaration, chooseDeclarationAt, type DeclarationTarget } from "@/features/navigation/chooseDeclaration";
import { noteLibrarySymbol } from "./library";

export const toDeclarationTargets = (targets: NavTarget[]): DeclarationTarget[] =>
  targets.map((t) => ({
    path: t.path,
    line: t.line,
    column: t.column,
    name: t.name || undefined,
    container: t.container,
  }));

/** Open a target; library files remember the symbol for their breadcrumbs. */
export function openTarget(t: DeclarationTarget): void {
  noteLibrarySymbol(t.path, t.name);
  void openFileAt(t.path, t.line + 1, (t.column ?? 0) + 1);
}

/** Open the only target or let the user choose below `pos`. */
export function chooseTargetAt(view: EditorView, pos: number, targets: NavTarget[], title?: string): void {
  chooseDeclarationAt(view, pos, toDeclarationTargets(targets), { title, open: openTarget });
}

/** Open the only target or let the user choose at screen position x/y (gutter icons). */
export function chooseTargetAtPoint(x: number, y: number, targets: NavTarget[], title?: string): void {
  chooseDeclaration(toDeclarationTargets(targets), { x, y, title, open: openTarget });
}
