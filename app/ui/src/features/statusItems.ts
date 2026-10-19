/** Status bar contribution point (right side widgets like branch, encoding, PHP version). */
import type { Component } from "solid-js";
import { createSignal } from "solid-js";

export interface StatusItem {
  id: string;
  align: "left" | "right";
  order: number;
  component: Component;
}

const [statusItems, setStatusItems] = createSignal<StatusItem[]>([]);

export function registerStatusItem(item: StatusItem): void {
  setStatusItems((prev) => [...prev.filter((p) => p.id !== item.id), item].sort((a, b) => a.order - b.order));
}

export { statusItems };
