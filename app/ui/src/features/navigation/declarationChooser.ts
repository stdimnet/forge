/**
 * PHPStorm's "Choose Declaration" popup: Go to Declaration (Ctrl+B / Ctrl+Click) and line
 * markers that find several targets list them at the caret instead of guessing. One shared
 * mechanism for every language pack (PHP and its frameworks, WebForge, LuaForge); the
 * targets come from Rust (language servers or pack indexes), the popup only renders them.
 */
import { createSignal } from "solid-js";
import type { EditorView } from "@codemirror/view";
import { openFileAt } from "@/editor/views";

export interface DeclarationTarget {
  /** Workspace-relative path. */
  path: string;
  /** 0-based line. */
  line: number;
  /** 0-based (UTF-16) column. */
  column?: number;
  /** Code of the target line, shown in the row (from the provider, e.g. Lua's index). */
  preview?: string;
  /** Symbol name shown instead of the file (`StripeGateway`, `Model::save()`). */
  name?: string;
  /** Greyed out after the name: namespace, declaring class or PHP extension. */
  container?: string | null;
  /** Group the target belongs to (Related Symbol: "Tests", "Views", ...); shown as a separator. */
  group?: string;
  /** The target is a whole file: no line number after the name. */
  wholeFile?: boolean;
}

export interface Chooser {
  title: string;
  x: number;
  y: number;
  targets: DeclarationTarget[];
  open: (t: DeclarationTarget) => void;
}

const [chooser, setChooser] = createSignal<Chooser | null>(null);
export { chooser };

export const closeChooser = () => setChooser(null);

export const revealTarget = (t: DeclarationTarget) => void openFileAt(t.path, t.line + 1, (t.column ?? 0) + 1);

/**
 * Open the only target, or let the user choose between several at screen position x/y.
 * `open` replaces the default navigation (e.g. a pack's own reveal).
 */
export function chooseDeclaration(
  targets: DeclarationTarget[],
  at: { x: number; y: number; title?: string; open?: (t: DeclarationTarget) => void },
): void {
  const open = at.open ?? revealTarget;
  if (targets.length === 1) open(targets[0]);
  else if (targets.length > 1)
    setChooser({ title: at.title ?? "Choose Declaration", x: at.x, y: at.y, targets, open });
}

/** {@link chooseDeclaration} below the editor position `pos`. */
export function chooseDeclarationAt(
  view: EditorView,
  pos: number,
  targets: DeclarationTarget[],
  opts: { title?: string; open?: (t: DeclarationTarget) => void } = {},
): void {
  const coords = view.coordsAtPos(pos);
  chooseDeclaration(targets, { x: coords?.left ?? 200, y: (coords?.bottom ?? 200) + 4, ...opts });
}
