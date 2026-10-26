/** Frame of the debugger dialogs (title, body, footer) on a backdrop. */
import { X } from "lucide-solid";

/** Escape closes the dialog it was pressed in (element-scoped, not a global shortcut). */
export const onEscape = (close: () => void) => (e: KeyboardEvent) => {
  if (e.key !== "Escape") return;
  e.preventDefault();
  e.stopPropagation();
  close();
};

export function DialogFrame(props: { title: string; class?: string; onClose: () => void; children: unknown; footer: unknown }) {
  return (
    <div class="dbg-backdrop" onMouseDown={(e) => e.target === e.currentTarget && props.onClose()} onKeyDown={onEscape(props.onClose)}>
      <div class={`dbg-dialog ${props.class ?? ""}`} role="dialog" aria-label={props.title}>
        <header class="dbg-dialog-title">
          <span>{props.title}</span>
          <button class="icon-btn" title="Close" onClick={props.onClose}>
            <X size={14} />
          </button>
        </header>
        <div class="dbg-dialog-body">{props.children as never}</div>
        <footer class="dbg-dialog-actions">{props.footer as never}</footer>
      </div>
    </div>
  );
}
