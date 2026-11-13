/**
 * Main toolbar contribution point: small widgets left of the Run/Debug configuration
 * selector, e.g. "Start Listening for PHP Debug Connections".
 */
import type { Component } from "solid-js";
import { createSignal } from "solid-js";

export interface ToolbarItem {
  id: string;
  order: number;
  component: Component;
}

const [toolbarItems, setToolbarItems] = createSignal<ToolbarItem[]>([]);

export function registerToolbarItem(item: ToolbarItem): void {
  setToolbarItems((prev) => [...prev.filter((p) => p.id !== item.id), item].sort((a, b) => a.order - b.order));
}

export { toolbarItems };
