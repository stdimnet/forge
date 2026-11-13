/** Status bar contribution point (right side widgets like branch, encoding, PHP version). */
import type { Component } from "solid-js";
import { createSignal } from "solid-js";

export interface StatusItem {
  id: string;
  align: "left" | "right";
  order: number;
  component: Component;
  /** Render only while this is true. */
  when?: () => boolean;
}

const [statusItems, setStatusItems] = createSignal<StatusItem[]>([]);

export function registerStatusItem(item: StatusItem): void {
  setStatusItems((prev) => [...prev.filter((p) => p.id !== item.id), item].sort((a, b) => a.order - b.order));
}

export { statusItems };

/** Items to render on one side, in order. */
export const visibleStatusItems = (align: StatusItem["align"]) =>
  statusItems().filter((s) => s.align === align && (s.when?.() ?? true));

/**
 * The status bar's message on the left (PHPStorm's status text), e.g. "3 usages of 'total'
 * found (Press Escape to remove the highlighting)". `null` clears it.
 */
const [statusMessage, setStatusMessage] = createSignal<string | null>(null);
export { statusMessage };

export function showStatusMessage(text: string | null): void {
  setStatusMessage(text);
}
