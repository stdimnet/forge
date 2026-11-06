/**
 * Popup of a status bar widget (Code With Me guest, remote host): opens above the widget like
 * PHPStorm's, shifts sideways to stay inside the window, flips below only when there is more
 * room there, and scrolls when the window is too low for it.
 *
 * It renders in a popup layer of the overlay stack (`layers.tsx`) with `position: fixed`, so
 * neither the status bar nor CodeMirror's sticky editor panels (e.g. the breadcrumbs above the
 * status bar) can cover or clip it.
 */
import { createSignal, onCleanup, onMount, type JSX } from "solid-js";
import { claimPopup } from "./popupGroup";
import { Layer } from "./layers";

/** Space between the popup and its widget, and between the popup and the window edges. */
const GAP = 6;
const MARGIN = 6;

export interface PopupPlacement {
  left: number;
  top: number;
  /** Height the popup may use; it scrolls beyond that. */
  maxHeight: number;
  above: boolean;
}

/**
 * Where a popup of `size` goes next to `anchor` inside a `viewport`: above the anchor (below
 * only when that side has more room), aligned with the anchor's left (`start`) or right
 * (`end`) edge and shifted to stay inside the viewport.
 */
export function placeStatusPopup(
  anchor: { left: number; right: number; top: number; bottom: number },
  size: { width: number; height: number },
  viewport: { width: number; height: number },
  align: "start" | "end" = "start",
): PopupPlacement {
  const roomAbove = anchor.top - GAP - MARGIN;
  const roomBelow = viewport.height - anchor.bottom - GAP - MARGIN;
  const above = size.height <= roomAbove || roomAbove >= roomBelow;
  const maxHeight = Math.max(0, above ? roomAbove : roomBelow);
  const height = Math.min(size.height, maxHeight);
  const top = above ? anchor.top - GAP - height : anchor.bottom + GAP;
  const wanted = align === "end" ? anchor.right - size.width : anchor.left;
  const left = Math.max(MARGIN, Math.min(wanted, viewport.width - size.width - MARGIN));
  return { left, top, maxHeight, above };
}

export function StatusPopup(props: {
  /** The status bar widget the popup belongs to; clicks on it do not close the popup. */
  anchor: HTMLElement;
  align?: "start" | "end";
  class: string;
  label: string;
  onClose: () => void;
  testId?: string;
  children: JSX.Element;
}) {
  let el: HTMLDivElement | undefined;
  const [place, setPlace] = createSignal<PopupPlacement | null>(null);
  const update = () => {
    if (!el) return;
    // Measure the natural height: a previous max-height must not limit it.
    const saved = el.style.maxHeight;
    el.style.maxHeight = "none";
    const size = { width: el.offsetWidth, height: el.offsetHeight };
    el.style.maxHeight = saved;
    const viewport = { width: window.innerWidth, height: window.innerHeight };
    setPlace(placeStatusPopup(props.anchor.getBoundingClientRect(), size, viewport, props.align));
  };
  const onDoc = (e: MouseEvent) => {
    const t = e.target as Node;
    if (!el?.contains(t) && !props.anchor.contains(t)) props.onClose();
  };
  onMount(() => {
    const release = claimPopup(() => props.onClose());
    update();
    const observer = new ResizeObserver(update);
    if (el) observer.observe(el);
    observer.observe(props.anchor);
    window.addEventListener("resize", update);
    document.addEventListener("mousedown", onDoc);
    onCleanup(() => {
      observer.disconnect();
      window.removeEventListener("resize", update);
      document.removeEventListener("mousedown", onDoc);
      release();
    });
  });
  return (
    <Layer kind="popup">
      <div
        ref={el}
        class={`status-popup-anchored ${props.class}`}
        classList={{ below: place()?.above === false }}
        role="dialog"
        aria-label={props.label}
        data-testid={props.testId}
        style={{
          left: `${place()?.left ?? 0}px`,
          top: `${place()?.top ?? 0}px`,
          "max-height": place() ? `${place()!.maxHeight}px` : undefined,
          visibility: place() ? "visible" : "hidden",
        }}
      >
        {props.children}
      </div>
    </Layer>
  );
}
