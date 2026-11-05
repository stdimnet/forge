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
  /** Default stripe; users can move the window (`state/layout.ts`: `moveToolWindowTo`). */
  anchor: ToolWindowAnchor;
  /**
   * Starts in the split group of its anchor (Left Bottom, Right Bottom, Bottom Right) like
   * PHPStorm's secondary tool windows.
   */
  split?: boolean;
  /** Default sort order inside its stripe group (lower first). */
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
  /**
   * A stand-in until the real feature registers ("… is coming soon"). It never replaces a
   * real window with the same id: production bundles can evaluate a feature module before
   * the placeholders, so the order of registration must not matter.
   */
  placeholder?: boolean;
}

const [toolWindows, setToolWindows] = createSignal<ToolWindow[]>([]);

export function registerToolWindow(tw: ToolWindow): void {
  setToolWindows((prev) => {
    if (tw.placeholder && prev.some((p) => p.id === tw.id && !p.placeholder)) return prev;
    return [...prev.filter((p) => p.id !== tw.id), tw].sort((a, b) => a.order - b.order);
  });
}

export { toolWindows };

const [filters, setFilters] = createSignal<((tw: ToolWindow) => boolean)[]>([]);

/** Hide tool windows for a reason that applies to all of them (e.g. a Code With Me guest). */
export function registerToolWindowFilter(keep: (tw: ToolWindow) => boolean): void {
  setFilters((f) => [...f, keep]);
}

/** Tool windows registered for `anchor` (their default place; see `stripeToolWindows`). */
export const toolWindowsFor = (anchor: ToolWindowAnchor) =>
  toolWindows().filter((t) => t.anchor === anchor && (t.when?.() ?? true) && filters().every((keep) => keep(t)));

export const findToolWindow = (id: string | null) =>
  id ? toolWindows().find((t) => t.id === id) : undefined;
