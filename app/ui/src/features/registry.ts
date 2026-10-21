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
  /** Optional header actions rendered at the right end of the header. */
  actions?: () => JSX.Element;
  /** Optional content right after the title, e.g. tabs (Terminal, Run). */
  headerContent?: () => JSX.Element;
  /** Show the window only while this is true (e.g. the project uses the framework). */
  when?: () => boolean;
  /**
   * Small marker on the stripe icon while the window is hidden, e.g. terminal progress.
   * Return nothing to show no marker.
   */
  badge?: () => JSX.Element;
}

const [toolWindows, setToolWindows] = createSignal<ToolWindow[]>([]);

export function registerToolWindow(tw: ToolWindow): void {
  setToolWindows((prev) => [...prev.filter((p) => p.id !== tw.id), tw].sort((a, b) => a.order - b.order));
}

export { toolWindows };

export const toolWindowsFor = (anchor: ToolWindowAnchor) =>
  toolWindows().filter((t) => t.anchor === anchor && (t.when?.() ?? true));

export const findToolWindow = (id: string | null) =>
  id ? toolWindows().find((t) => t.id === id) : undefined;
