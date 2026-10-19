import { createStore } from "solid-js/store";
import type { ToolWindowAnchor } from "@/features/registry";

interface LayoutState {
  /** Active tool window per anchor (one per side, like PHPStorm). */
  active: Record<ToolWindowAnchor, string | null>;
  size: Record<ToolWindowAnchor, number>;
}

const [layout, setLayout] = createStore<LayoutState>({
  active: { left: "project", right: null, bottom: null },
  size: { left: 300, right: 380, bottom: 240 },
});

export { layout };

/** Toggle a tool window: clicking the active one hides its side. */
export function toggleToolWindow(anchor: ToolWindowAnchor, id: string): void {
  setLayout("active", anchor, (cur) => (cur === id ? null : id));
}

export function showToolWindow(anchor: ToolWindowAnchor, id: string): void {
  setLayout("active", anchor, id);
}

export function resizePanel(anchor: ToolWindowAnchor, size: number): void {
  const limits = anchor === "bottom" ? [100, 700] : [180, 900];
  setLayout("size", anchor, Math.max(limits[0], Math.min(limits[1], size)));
}
