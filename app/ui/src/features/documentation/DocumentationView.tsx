/**
 * Documentation tool window (View | Tool Windows | Documentation): the documentation of the
 * last symbol, opened from the Quick Documentation popup (pin, or Ctrl+Q a second time).
 * With "Auto-Update from Source" it follows the caret.
 */
import { createSignal, Show } from "solid-js";
import { Crosshair, ExternalLink, Pencil } from "lucide-solid";
import { shortcutLabel } from "@/keymap";
import type { Documentation } from "./docs";

const [shown, setShown] = createSignal<Documentation | null>(null);
const [autoUpdate, setAutoUpdate] = createSignal(true);

export { autoUpdate, setShown, shown };

/** Actions on the documentation shown (External Documentation, Edit Source). */
export interface DocumentationActions {
  external: (doc: Documentation) => void;
  editSource: (doc: Documentation) => void;
}

let docActions: DocumentationActions | null = null;
export const setDocumentationActions = (a: DocumentationActions) => (docActions = a);

const withShortcut = (title: string, actionId: string) => {
  const s = shortcutLabel(actionId);
  return s ? `${title} (${s})` : title;
};

export function DocumentationActionsBar() {
  return (
    <>
      <button
        class="icon-btn"
        classList={{ active: autoUpdate() }}
        title="Auto-Update from Source"
        aria-pressed={autoUpdate()}
        onClick={() => setAutoUpdate(!autoUpdate())}
      >
        <Crosshair size={15} />
      </button>
      <button
        class="icon-btn"
        title={withShortcut("External Documentation", "ExternalJavaDoc")}
        disabled={!shown()}
        onClick={() => shown() && docActions?.external(shown()!)}
      >
        <ExternalLink size={15} />
      </button>
      <button
        class="icon-btn"
        title={withShortcut("Edit Source", "EditSource")}
        disabled={!shown()}
        onClick={() => shown() && docActions?.editSource(shown()!)}
      >
        <Pencil size={15} />
      </button>
    </>
  );
}

export function DocumentationView() {
  return (
    <div class="doc-view" tabIndex={-1}>
      <Show
        when={shown()}
        keyed
        fallback={
          <div class="doc-view-empty muted">
            Place the caret on a symbol to see its documentation, or press {shortcutLabel("QuickJavaDoc") || "Ctrl+Q"}
          </div>
        }
      >
        {(doc) => <div class="doc-view-body" ref={(el) => el.append(doc.render())} />}
      </Show>
    </div>
  );
}
