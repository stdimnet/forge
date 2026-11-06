/**
 * The overlay stack: one stacking order for every popup, menu, dialog and notification.
 *
 *   <Layer kind="popup">
 *     <div class="my-popup">…</div>
 *   </Layer>
 *
 * - Every layer has a kind. Its base is the matching `--z-*` variable of `theme/theme.css`
 *   (mirrored in `LAYER_BASE`), lowest first: editor overlays, floating tool windows, popups,
 *   modal dialogs, notifications, context menus, tooltips, drag ghosts, system.
 * - A layer opened from another one (its parent: the `<Layer>` around it, or the layer that
 *   had focus when it opened) sits above that parent, whatever its kind: a popup or a menu
 *   opened from a dialog is always above the dialog, a dialog opened from Settings is above
 *   Settings.
 * - The most recently opened layer is above every earlier layer of the same or a lower kind,
 *   so the last popup is on top, as in PHPStorm. Closing a layer releases its place.
 * - `<Layer>` renders its content through a `<Portal>` into one overlay host at the end of
 *   `<body>`, inside a wrapper that carries the layer's z-index: a parent that creates its
 *   own stacking context cannot trap the popup.
 *
 * Dialogs and popups that features render themselves (a `*-backdrop` element, a fixed
 * `role="dialog"`, `role="menu"` or `.popup`) are adopted by the dialog accessibility manager
 * (`features/accessibility/dialogs.ts`) when they appear, with `adoptOverlay`, so they take
 * part in the same order without code of their own.
 *
 * Inside a component, small z-index values (1 to 10) stay fine for local overlap (a sticky
 * header over a list); the component root then gets `isolation: isolate`.
 */
import { createContext, onCleanup, useContext, type JSX } from "solid-js";
import { Portal } from "solid-js/web";

export const LAYER_KINDS = [
  "editor-overlay",
  "floating-tool-window",
  "popup",
  "modal",
  "notification",
  "context-menu",
  "tooltip",
  "drag-ghost",
  "system",
] as const;

export type LayerKind = (typeof LAYER_KINDS)[number];

/** The `--z-<kind>` values of `theme.css` (a test keeps both in sync). */
export const LAYER_BASE: Record<LayerKind, number> = {
  "editor-overlay": 300,
  "floating-tool-window": 400,
  popup: 500,
  modal: 600,
  notification: 700,
  "context-menu": 800,
  tooltip: 900,
  "drag-ghost": 1000,
  system: 1100,
};

export interface Layer {
  readonly id: number;
  readonly kind: LayerKind;
  /** The z-index of the layer's root element. */
  readonly z: number;
  readonly parent: Layer | null;
  /** False once closed. */
  readonly isOpen: boolean;
  /** Release the layer's place in the stack (idempotent). */
  close(): void;
}

const stack: Layer[] = [];
let seq = 0;

/**
 * The z-index a new layer of `kind` gets: above its base, above its parent and above every
 * open layer of the same or a lower kind; below the system layer unless it is one.
 */
export function layerZ(kind: LayerKind, parent: Pick<Layer, "z"> | null, open: readonly Pick<Layer, "kind" | "z">[]): number {
  const base = LAYER_BASE[kind];
  // One above the base: elements that use the bare `--z-*` variable stay below layers.
  let z = base + 1;
  if (parent) z = Math.max(z, parent.z + 1);
  for (const l of open) if (LAYER_BASE[l.kind] <= base) z = Math.max(z, l.z + 1);
  return kind === "system" ? z : Math.min(z, LAYER_BASE.system - 1);
}

/** Register a layer of `kind` (above `parent`, if given); call `close()` when it goes away. */
export function openLayer(kind: LayerKind, parent: Layer | null = null): Layer {
  const live = parent?.isOpen ? parent : null;
  let isOpen = true;
  const layer: Layer = {
    id: ++seq,
    kind,
    z: layerZ(kind, live, stack),
    parent: live,
    get isOpen() {
      return isOpen;
    },
    close() {
      if (!isOpen) return;
      isOpen = false;
      const i = stack.indexOf(layer);
      if (i >= 0) stack.splice(i, 1);
    },
  };
  stack.push(layer);
  return layer;
}

/** The open layers, in the order they opened (for tests and diagnostics). */
export const openLayers = (): readonly Layer[] => [...stack];

/** The open layer an element belongs to (its own or an ancestor's), if any. */
export function layerOf(el: Element | null | undefined): Layer | undefined {
  const host = el?.closest?.<HTMLElement>("[data-layer-id]");
  const id = Number(host?.dataset.layerId);
  return id ? stack.find((l) => l.id === id) : undefined;
}

/** The z-index of the layer `el` belongs to, or `null` outside the overlay stack. */
export function layerZOf(el: Element | null | undefined): number | null {
  return layerOf(el)?.z ?? null;
}

function mark(el: HTMLElement, layer: Layer): void {
  el.dataset.layerId = String(layer.id);
  el.dataset.layerKind = layer.kind;
  el.style.zIndex = String(layer.z);
}

/** The element all `<Layer>`s render into: a plain `<div>` at the end of `<body>`. */
export function overlayHost(doc: Document = document): HTMLElement {
  let host = doc.getElementById("forge-overlays");
  if (!host) {
    host = doc.createElement("div");
    host.id = "forge-overlays";
    doc.body.appendChild(host);
  }
  return host;
}

const LayerContext = createContext<Layer>();

/** The layer the calling component renders in, if any. */
export const useLayer = (): Layer | undefined => useContext(LayerContext);

/**
 * Render `children` in a new layer of `kind`, through a `<Portal>` into the overlay host.
 * The layer opens with the component and closes when it unmounts. Its parent is `parent`,
 * else the `<Layer>` around it, else the layer of the focused element (the opener).
 */
export function Layer(props: { kind: LayerKind; parent?: Layer | null; children: JSX.Element }) {
  const around = useLayer();
  const parent =
    props.parent !== undefined ? props.parent : (around ?? layerOf(typeof document === "undefined" ? null : document.activeElement) ?? null);
  const layer = openLayer(props.kind, parent);
  onCleanup(() => layer.close());
  return (
    <LayerContext.Provider value={layer}>
      <Portal
        mount={overlayHost()}
        ref={(el: HTMLDivElement) => {
          el.className = "forge-layer";
          mark(el, layer);
        }}
      >
        {props.children}
      </Portal>
    </LayerContext.Provider>
  );
}

/**
 * Register a layer for the calling component without moving it (a floating tool window that
 * stays in the workbench); the returned layer closes with the component. Put `layer.z` on
 * the component's root, e.g. with `layerStyle`.
 */
export function createLayer(kind: LayerKind, parent?: Layer | null): Layer {
  const layer = openLayer(kind, parent !== undefined ? parent : (useLayer() ?? null));
  onCleanup(() => layer.close());
  return layer;
}

/** Inline style that puts an element on its layer. */
export const layerStyle = (layer: Layer): { "z-index": string } => ({ "z-index": String(layer.z) });

// ---- Overlays rendered by features (adopted by the dialog accessibility manager) --------

const adopted = new Map<HTMLElement, Layer>();

/**
 * Put an overlay root that a feature renders itself (a backdrop, a fixed popup or menu) on
 * the stack, above the layer of `opener`. Elements inside a layer, and elements that are not
 * `position: fixed` (local overlays), are left alone. Returns the layer, if one was opened.
 */
export function adoptOverlay(root: HTMLElement, kind: LayerKind, opener: Element | null): Layer | null {
  if (!root.isConnected || adopted.has(root) || root.closest("[data-layer-id]")) return null;
  if (root.ownerDocument.defaultView?.getComputedStyle(root).position !== "fixed") return null;
  const layer = openLayer(kind, layerOf(opener) ?? null);
  adopted.set(root, layer);
  mark(root, layer);
  return layer;
}

/** Release adopted overlays that are `node` or inside it (they were removed). */
export function releaseOverlaysIn(node: Node): void {
  for (const [root, layer] of adopted) {
    if (root === node || node.contains(root)) {
      adopted.delete(root);
      layer.close();
    }
  }
}
