/**
 * Clipboard: records copies and cuts for Paste from History (`PasteMultiple`,
 * Ctrl+Shift+V), pastes with one part per caret after a multi-caret copy, and Paste as
 * Plain Text (`PasteSimple`). Copy, Cut and Paste themselves are native editor keys; the
 * menu entries live in `features/contextMenu`.
 */
import { render } from "solid-js/web";
import { EditorView } from "@codemirror/view";
import { registerEditorExtension } from "@/editor/extensions";
import { activeView } from "@/editor/views";
import { actionById, showBalloon } from "@/keymap";
import { registerCommand } from "@/state/commands";
import { copiedFrom, needsCaretPaste, pasteInto, recordCopy, setLastCopy } from "./clipboard";
import { PasteFromHistoryHost, choosePasteContent } from "./PasteFromHistory";
import { t as tr } from "@/i18n";
import "./clipboard.css";

registerEditorExtension(() => [
  // Observers run before CodeMirror's own copy/cut handler, so the selection is still there.
  EditorView.domEventObservers({
    copy: (_e, view) => record(view),
    cut: (_e, view) => record(view),
  }),
  EditorView.domEventHandlers({
    paste: (event, view) => {
      const text = event.clipboardData?.getData("text/plain");
      if (!text || !needsCaretPaste(view.state, text)) return false;
      pasteInto(view, text);
      return true;
    },
  }),
]);

function record(view: EditorView): void {
  const copied = copiedFrom(view.state);
  recordCopy(copied.text, copied.linewise ? [] : copied.carets);
}

type TextField = HTMLInputElement | HTMLTextAreaElement;
const isTextField = (el: Element | null): el is TextField =>
  el instanceof HTMLTextAreaElement || (el instanceof HTMLInputElement && /^(text|search|url|email|tel|password|)$/.test(el.type));

if (typeof document !== "undefined") {
  // Copies outside code editors (text fields, read-only text) count too, as in PHPStorm.
  const onCopy = (e: ClipboardEvent) => {
    const target = e.target instanceof Element ? e.target : null;
    if (target?.closest(".cm-editor")) return;
    const field = document.activeElement;
    const text = isTextField(field) && field.type !== "password"
      ? field.value.slice(field.selectionStart ?? 0, field.selectionEnd ?? 0)
      : (document.getSelection()?.toString() ?? "");
    recordCopy(text);
  };
  document.addEventListener("copy", onCopy, true);
  document.addEventListener("cut", onCopy, true);

  const host = document.createElement("div");
  host.id = "paste-from-history";
  document.body.appendChild(host);
  render(() => <PasteFromHistoryHost />, host);
}

/** The editor (or text field) that had the focus when the popup opened. */
function pasteTarget(): { view: EditorView } | { field: TextField } | null {
  const focused = document.activeElement;
  const editorDom = focused?.closest(".cm-editor");
  const view = editorDom ? EditorView.findFromDOM(editorDom as HTMLElement) : null;
  if (view) return { view };
  if (isTextField(focused)) return { field: focused };
  const active = activeView();
  return active ? { view: active } : null;
}

type Target = NonNullable<ReturnType<typeof pasteTarget>>;
const refocus = (t: Target) => ("view" in t ? t.view.focus() : t.field.focus());

async function pasteFromHistory(): Promise<void> {
  const target = pasteTarget();
  const entry = await choosePasteContent();
  if (!entry || !target) {
    if (target) refocus(target);
    return;
  }
  setLastCopy(entry);
  // The chosen entry becomes the clipboard content, like in PHPStorm.
  void navigator.clipboard?.writeText(entry.text).catch(() => undefined);
  refocus(target);
  if ("view" in target) {
    pasteInto(target.view, entry.text, entry.carets);
  } else {
    const { field } = target;
    field.setRangeText(entry.text, field.selectionStart ?? field.value.length, field.selectionEnd ?? field.value.length, "end");
    field.dispatchEvent(new Event("input", { bubbles: true }));
  }
}

registerCommand({ id: "PasteMultiple", title: actionById("PasteMultiple")?.title ?? "Paste from History…", run: pasteFromHistory });

/** Paste the system clipboard into the active editor (menus, Paste as Plain Text). */
export async function pasteClipboard(): Promise<void> {
  const view = activeView();
  if (!view) return;
  let text: string;
  try {
    text = await navigator.clipboard.readText();
  } catch {
    showBalloon(tr("tools.clipboard.clipboardIsNotAvailable"), tr("tools.clipboard.useTheKeyboardShortcutInstead"));
    return;
  }
  pasteInto(view, text);
  view.focus();
}
