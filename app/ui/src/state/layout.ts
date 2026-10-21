import { createStore } from "solid-js/store";
import type { ToolWindowAnchor } from "@/features/registry";

interface LayoutState {
  /** Active tool window per anchor (one per side, like PHPStorm). */
  active: Record<ToolWindowAnchor, string | null>;
  size: Record<ToolWindowAnchor, number>;
  /** Selected tab inside tool windows that have tabs (tool window id → tab id). */
  tabs: Record<string, string>;
}

/** Size limits of the tool window areas, as `forge_core::project_state` enforces them. */
export const SIZE_LIMITS: Record<ToolWindowAnchor, [number, number]> = {
  left: [180, 900],
  right: [180, 900],
  bottom: [100, 700],
};

const [layout, setLayout] = createStore<LayoutState>({
  active: { left: "project", right: null, bottom: null },
  size: { left: 300, right: 380, bottom: 240 },
  tabs: {},
});

export { layout, setLayout };

/** Toggle a tool window: clicking the active one hides its side. */
export function toggleToolWindow(anchor: ToolWindowAnchor, id: string): void {
  setLayout("active", anchor, (cur) => (cur === id ? null : id));
}

export function showToolWindow(anchor: ToolWindowAnchor, id: string): void {
  setLayout("active", anchor, id);
}

export function resizePanel(anchor: ToolWindowAnchor, size: number): void {
  const [min, max] = SIZE_LIMITS[anchor];
  setLayout("size", anchor, Math.max(min, Math.min(max, size)));
}

/** The selected tab of a tool window with tabs (restored with the project's layout). */
export const toolWindowTab = (id: string, fallback: string): string => layout.tabs[id] ?? fallback;

export function setToolWindowTab(id: string, tab: string): void {
  setLayout("tabs", id, tab);
}
