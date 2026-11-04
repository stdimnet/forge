/**
 * Dialog accessibility for every dialog in the window, without each feature wiring it up:
 *
 * - Elements with `role="dialog"` (and the content of known backdrops: `.popup-backdrop`,
 *   `.modal-backdrop`, `*-backdrop`) get a name from their title when they have none, and
 *   `aria-modal="true"` when a backdrop blocks the window behind them.
 * - Focus stays in the topmost modal dialog: Tab past its last control wraps to the first,
 *   Shift+Tab before the first wraps to the last (two focus guards at the ends of <body>
 *   catch focus leaving the document).
 * - When a dialog closes and focus would be lost, it returns to the element that had focus
 *   when the dialog opened, or to the editor.
 *
 * Popups that belong to a dialog (menus, list popups, completion) may take focus outside it.
 */

const DIALOG = '[role="dialog"], [role="alertdialog"]';
const BACKDROP = '.popup-backdrop, .modal-backdrop, [class*="-backdrop"], .ssh-overlay';
/** Focus may go here even while a modal dialog is open. */
const ALLOWED_OUTSIDE =
  '[role="menu"], [role="listbox"], .context-menu, .cm-tooltip, [data-focus-trap-allow], [data-live-region], [data-presentation-assistant]';
const TABBABLE =
  'a[href], button:not([disabled]), input:not([disabled]):not([type="hidden"]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"]), [contenteditable="true"], [contenteditable=""]';

interface Open {
  el: HTMLElement;
  opener: HTMLElement | null;
}

const open: Open[] = [];
let lastFocusOutside: HTMLElement | null = null;
let installedOn: Document | null = null;

/** Controls Tab reaches inside `root`, in DOM order (hidden ones left out). */
export function tabbables(root: ParentNode): HTMLElement[] {
  return [...root.querySelectorAll<HTMLElement>(TABBABLE)].filter((el) => {
    if (el.closest("[inert], [aria-hidden='true']") || el.dataset.focusGuard !== undefined) return false;
    // jsdom has no layout: treat everything as visible there.
    const visible = el.getClientRects().length > 0 || navigator.userAgent.includes("jsdom");
    return visible && !el.closest(".hidden");
  });
}

/** Whether the dialog blocks the window behind it. */
function isModal(el: HTMLElement): boolean {
  const flag = el.getAttribute("aria-modal");
  if (flag) return flag === "true";
  return !!el.closest(BACKDROP) || !!el.parentElement?.matches(BACKDROP);
}

let idSeq = 0;
const ensureId = (el: HTMLElement) => (el.id ||= `forge-a11y-${++idSeq}`);

/** Name a dialog after its title and mark it modal; idempotent. */
export function describeDialog(el: HTMLElement): void {
  if (!el.matches(DIALOG)) el.setAttribute("role", "dialog");
  if (!el.hasAttribute("aria-label") && !el.hasAttribute("aria-labelledby")) {
    const title = el.querySelector<HTMLElement>(
      ".dialog-title, .window-title, [class$='-title'], [class*='-title '], h1, h2, h3, header",
    );
    if (title?.textContent?.trim()) el.setAttribute("aria-labelledby", ensureId(title));
  }
  if (!el.hasAttribute("aria-modal") && isModal(el)) el.setAttribute("aria-modal", "true");
  labelControls(el);
}

/**
 * Connect form labels to the control right after them (`<label>Name:</label><input>`), the
 * common dialog layout, when the code did not do it with `for`.
 */
export function labelControls(root: ParentNode): void {
  for (const label of root.querySelectorAll<HTMLLabelElement>("label:not([for])")) {
    if (label.querySelector("input, select, textarea")) continue;
    const next = label.nextElementSibling as HTMLInputElement | null;
    if (!next?.matches("input, select, textarea")) continue;
    if (next.hasAttribute("aria-label") || next.hasAttribute("aria-labelledby") || next.labels?.length) continue;
    label.htmlFor = ensureId(next);
  }
}

/** The dialog element a backdrop holds (its first element child), when it names none itself. */
function dialogOfBackdrop(backdrop: HTMLElement): HTMLElement | null {
  if (backdrop.matches(DIALOG) || backdrop.querySelector(DIALOG)) return null;
  return backdrop.firstElementChild as HTMLElement | null;
}

const topModal = (): Open | undefined => [...open].reverse().find((o) => o.el.isConnected && isModal(o.el));

function track(el: HTMLElement): void {
  if (open.some((o) => o.el === el)) return;
  describeDialog(el);
  const active = el.ownerDocument.activeElement as HTMLElement | null;
  const opener = active && active !== el.ownerDocument.body && !el.contains(active) ? active : lastFocusOutside;
  open.push({ el, opener });
  // Content shown a moment later (resources, `Show`) gets its labels too.
  setTimeout(() => el.isConnected && labelControls(el), 120);
  // Dialogs usually focus their first field themselves; otherwise focus the first control.
  setTimeout(() => {
    if (!el.isConnected || el.contains(el.ownerDocument.activeElement)) return;
    if (topModal()?.el !== el) return;
    (tabbables(el)[0] ?? focusSelf(el)).focus();
  }, 60);
}

function focusSelf(el: HTMLElement): HTMLElement {
  if (!el.hasAttribute("tabindex")) el.tabIndex = -1;
  return el;
}

/** Where focus goes when nothing else wants it: the active editor. */
function editorFallback(doc: Document): HTMLElement | null {
  return doc.querySelector<HTMLElement>(".editor-slot:not(.hidden) .cm-content");
}

function untrack(el: HTMLElement): void {
  const i = open.findIndex((o) => o.el === el);
  if (i < 0) return;
  const [{ opener }] = open.splice(i, 1);
  const doc = el.ownerDocument;
  // Let the closing code move focus itself first (e.g. back to a tree row).
  const lost = () => !doc.activeElement || doc.activeElement === doc.body || !doc.activeElement.isConnected;
  queueMicrotask(() => {
    if (!lost()) return;
    const target = opener?.isConnected ? opener : (topModal()?.el ?? editorFallback(doc));
    target?.focus();
  });
  // What the dialog did may replace the focused element a moment later (a file it opened
  // replaces the editor): then the editor gets focus, as in PHPStorm.
  setTimeout(() => {
    if (lost()) (topModal()?.el ?? editorFallback(doc))?.focus();
  }, 120);
}

/** Scan added or removed nodes for dialogs. */
function scan(nodes: NodeList, added: boolean): void {
  for (const n of nodes) {
    if (n.nodeType !== 1 /* element */) continue;
    const node = n as HTMLElement;
    const found = new Set<HTMLElement>();
    if (node.matches(DIALOG)) found.add(node);
    node.querySelectorAll<HTMLElement>(DIALOG).forEach((d) => found.add(d));
    if (added) {
      const backdrops = [...(node.matches(BACKDROP) ? [node] : []), ...node.querySelectorAll<HTMLElement>(BACKDROP)];
      for (const b of backdrops) {
        const d = dialogOfBackdrop(b);
        if (d) found.add(d);
      }
      found.forEach(track);
    } else {
      for (const o of [...open]) if (o.el === node || node.contains(o.el)) untrack(o.el);
    }
  }
}

function onFocusIn(e: FocusEvent): void {
  const target = e.target as HTMLElement | null;
  if (!target || target.nodeType !== 1 /* element */ || !target.dataset) return;
  const top = topModal();
  if (target.dataset.focusGuard !== undefined) {
    // Focus left the document's last (or first) control: wrap inside the dialog, or into the
    // window again.
    const root = top?.el ?? target.ownerDocument.body;
    const list = tabbables(root);
    // Coming from outside the document (no element lost focus), the guard is entered from
    // its own side: the start guard leads to the first control, the end guard to the last.
    const fromOutside = !e.relatedTarget;
    const wrapToLast = (target.dataset.focusGuard === "start") !== fromOutside;
    const next = wrapToLast ? list[list.length - 1] : list[0];
    (next ?? (top ? focusSelf(top.el) : null))?.focus();
    return;
  }
  if (!top) {
    if (!target.closest(DIALOG)) lastFocusOutside = target;
    return;
  }
  if (top.el.contains(target) || target.closest(ALLOWED_OUTSIDE) || target.closest(DIALOG)) return;
  // Tab moved past the dialog's end (to something after it) or before its start: wrap.
  const after = top.el.compareDocumentPosition(target) & 4 /* DOCUMENT_POSITION_FOLLOWING */;
  const list = tabbables(top.el);
  const next = after ? list[0] : list[list.length - 1];
  (next ?? focusSelf(top.el)).focus();
}

function guard(doc: Document, where: "start" | "end"): HTMLElement {
  const el = doc.createElement("div");
  el.tabIndex = 0;
  el.dataset.focusGuard = where;
  el.className = "sr-only";
  return el;
}

/** Start managing dialogs in `doc` (once per document). */
export function installDialogAccessibility(doc: Document = document): void {
  if (installedOn === doc) return;
  installedOn = doc;
  const start = guard(doc, "start");
  const end = guard(doc, "end");
  doc.body.prepend(start);
  doc.body.append(end);
  doc.addEventListener("focusin", onFocusIn, true);
  new MutationObserver((records) => {
    for (const r of records) {
      scan(r.removedNodes, false);
      scan(r.addedNodes, true);
    }
    // Hosts appended later (popups, dialogs) stay between the guards.
    if (doc.body.firstChild !== start) doc.body.prepend(start);
    if (doc.body.lastChild !== end) doc.body.append(end);
  }).observe(doc.body, { childList: true, subtree: true });
  scan(doc.body.childNodes, true);
}

/** The dialogs being managed, topmost last (for tests). */
export const openDialogs = (): HTMLElement[] => open.filter((o) => o.el.isConnected).map((o) => o.el);
