/**
 * Tool window registry. Every panel in the IDE (Project, Commit, Terminal, AI, ...) is
 * contributed by a feature module calling `registerToolWindow`, the UI equivalent of a
 * plugin contribution point.
 */
import type { Component, JSX } from "solid-js";
import { createSignal } from "solid-js";

export type ToolWindowAnchor = "left" | "right" | "bottom";

export interface ToolWindow {
  id: string;
  title: string;
  icon: Component<{ size?: number; "stroke-width"?: number }>;
  anchor: ToolWindowAnchor;
  /** Sort order inside its stripe (lower first). */
  order: number;
  component: Component;
  /** PHPStorm-style shortcut hint shown in tooltips, e.g. "Alt+1". */
  shortcut?: string;
  /** Optional header actions rendered right of the title. */
  actions?: () => JSX.Element;
}

const [toolWindows, setToolWindows] = createSignal<ToolWindow[]>([]);

export function registerToolWindow(tw: ToolWindow): void {
  setToolWindows((prev) => [...prev.filter((p) => p.id !== tw.id), tw].sort((a, b) => a.order - b.order));
}

export { toolWindows };

export const toolWindowsFor = (anchor: ToolWindowAnchor) =>
  toolWindows().filter((t) => t.anchor === anchor);

export const findToolWindow = (id: string | null) =>
  id ? toolWindows().find((t) => t.id === id) : undefined;
