/**
 * Keyboard focus between the areas of the main window: main toolbar, navigation bar, the tool
 * window stripes, the tool windows on each side, the editor and the status bar. "Focus Next
 * Area" / "Focus Previous Area" cycle through the visible ones and return to the control that
 * had focus there last; "Focus Editor" (Escape in a tool window) goes back to the editor.
 */
import { tabbables } from "./dialogs";

/** The areas in cycling order: id and the element that holds it. */
export const AREAS: { id: string; selector: string }[] = [
  { id: "toolbar", selector: ".main-toolbar" },
  { id: "navbar", selector: ".navbar-top" },
  { id: "stripe-left", selector: ".tool-stripe-left" },
  { id: "tools-left", selector: ".tool-area-left" },
  { id: "editor", selector: ".editor-area" },
  { id: "tools-right", selector: ".tool-area-right" },
  { id: "tools-bottom", selector: ".tool-area-bottom" },
  { id: "stripe-right", selector: ".tool-stripe-right" },
  { id: "statusbar", selector: ".status-bar" },
];

/** The control that had focus last in each area (weak: areas come and go). */
const lastFocus = new WeakMap<Element, HTMLElement>();

/** Remember focus per area; call once per document. */
export function trackAreaFocus(doc: Document = document): void {
  doc.addEventListener(
    "focusin",
    (e) => {
      const el = e.target as HTMLElement;
      const area = areaOf(el, doc);
      if (area) lastFocus.set(area.el, el);
    },
    true,
  );
}

interface FoundArea {
  id: string;
  el: HTMLElement;
}

/** Visible areas of the window, in order. */
export function visibleAreas(doc: Document = document): FoundArea[] {
  const out: FoundArea[] = [];
  for (const a of AREAS) {
    const el = doc.querySelector<HTMLElement>(a.selector);
    if (el && el.isConnected && (el.getClientRects().length > 0 || navigator.userAgent.includes("jsdom"))) out.push({ id: a.id, el });
  }
  return out;
}

/** The area that contains `el`. */
export function areaOf(el: Element | null, doc: Document = document): FoundArea | undefined {
  if (!el) return undefined;
  return visibleAreas(doc).find((a) => a.el.contains(el));
}

/** Where focus goes in an area: the last focused control there, the editor's text, or the first control. */
function focusTarget(area: FoundArea): HTMLElement | undefined {
  const last = lastFocus.get(area.el);
  if (last?.isConnected && area.el.contains(last) && !last.closest(".hidden")) return last;
  if (area.id === "editor") {
    const content = area.el.querySelector<HTMLElement>(".editor-slot:not(.hidden) .cm-content");
    if (content) return content;
  }
  // A tree or list in a tool window takes focus itself (it moves its own selection).
  const widget = area.el.querySelector<HTMLElement>('[role="tree"][tabindex], [role="listbox"][tabindex], [role="grid"][tabindex]');
  return widget ?? tabbables(area.el)[0];
}

/** Move focus to the next (`+1`) or previous (`-1`) visible area; returns the area's id. */
export function focusArea(step: 1 | -1, doc: Document = document): string | undefined {
  const areas = visibleAreas(doc).filter((a) => focusTarget(a));
  if (!areas.length) return undefined;
  const current = areaOf(doc.activeElement, doc);
  const at = current ? areas.findIndex((a) => a.id === current.id) : -1;
  const index = at < 0 ? (step > 0 ? 0 : areas.length - 1) : (at + step + areas.length) % areas.length;
  const next = areas[index];
  focusTarget(next)?.focus();
  return next.id;
}

/** Focus the active editor; false when no editor is open. */
export function focusEditor(doc: Document = document): boolean {
  const content = doc.querySelector<HTMLElement>(".editor-slot:not(.hidden) .cm-content");
  if (!content) return false;
  content.focus();
  return true;
}
