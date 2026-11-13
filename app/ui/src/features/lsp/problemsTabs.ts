/**
 * Tabs that features add to the Problems tool window after "Current File" and "Project
 * Errors", like IntelliJ's "Vulnerable Dependencies" tab (Composer audit).
 */
import { createSignal, type Component } from "solid-js";

export interface ProblemsTabDef {
  id: string;
  title: string;
  order?: number;
  /** Shown as a badge when it is more than zero. */
  count?: () => number;
  /** Hidden while this returns false (e.g. the plugin is not active). */
  visible?: () => boolean;
  component: Component;
}

const [tabs, setTabs] = createSignal<ProblemsTabDef[]>([]);

export function registerProblemsTab(tab: ProblemsTabDef): void {
  setTabs((prev) => [...prev.filter((t) => t.id !== tab.id), tab].sort((a, b) => (a.order ?? 0) - (b.order ?? 0)));
}

export const problemsTabs = () => tabs().filter((t) => t.visible?.() ?? true);
