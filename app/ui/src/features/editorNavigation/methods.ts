/**
 * Next Method / Previous Method (Alt+Down / Alt+Up, PHPStorm's `MethodDown` / `MethodUp`):
 * move the caret to the next or previous class, function, method or CSS rule header, and to
 * the closing brace of classes. The stops come from the Rust structure scopes
 * (`forge_codeintel::method_target`), so every language with Sticky Lines works: PHP,
 * JavaScript/TypeScript, Vue/HTML scripts, CSS/SCSS/Less, Lua and other brace languages.
 */
import { EditorView } from "@codemirror/view";
import { registerCommand } from "@/state/commands";
import { editor } from "@/state/editor";
import { actionById } from "@/keymap";
import { languageIdFor } from "@/editor/languages";
import { activeView } from "@/features/navigation/editorCommands";
import { callWasmSync, forgeWasm } from "@/wasm";

interface Stop {
  line: number;
  column: number;
}

async function moveToMethod(forward: boolean): Promise<void> {
  const path = editor.active;
  const view = activeView();
  if (!path || !view) return;
  const w = await forgeWasm();
  const { state } = view;
  const head = state.selection.main.head;
  const line = state.doc.lineAt(head);
  const language = languageIdFor(path, state.doc.toString()) ?? "";
  let stop: Stop | null = null;
  try {
    stop = callWasmSync<Stop | null>(w.editor_method_target, {
      text: state.doc.toString(),
      language,
      line: line.number - 1,
      column: head - line.from,
      forward,
    });
  } catch (e) {
    console.warn("next method:", e);
  }
  if (!stop || stop.line >= view.state.doc.lines) return;
  const target = view.state.doc.line(stop.line + 1);
  const pos = Math.min(target.from + stop.column, target.to);
  view.dispatch({ selection: { anchor: pos }, effects: EditorView.scrollIntoView(pos, { y: "nearest", yMargin: 48 }), userEvent: "select" });
}

registerCommand({ id: "MethodDown", title: actionById("MethodDown")?.title ?? "Next Method", category: "Navigate", run: () => moveToMethod(true) });
registerCommand({ id: "MethodUp", title: actionById("MethodUp")?.title ?? "Previous Method", category: "Navigate", run: () => moveToMethod(false) });
