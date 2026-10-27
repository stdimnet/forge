/**
 * Extra tabs of the Debug tool window, next to Threads & Variables and Console: packs add
 * views of their own for the sessions of their debugger (KotlinForge's Coroutines).
 */
import { createRoot, createSignal, type Component } from "solid-js";
import type { SessionInfo } from "./api";

export interface DebugTabDef {
  id: string;
  title: string;
  /** Whether the tab applies to the active session. */
  when: (session: SessionInfo | null | undefined) => boolean;
  component: Component;
}

const state = createRoot(() => {
  const [tabs, setTabs] = createSignal<DebugTabDef[]>([]);
  return { tabs, setTabs };
});

export const debugTabs = state.tabs;

/** Add (or replace) a Debug tool window tab. Returns a function that removes it. */
export function registerDebugTab(tab: DebugTabDef): () => void {
  state.setTabs((prev) => [...prev.filter((t) => t.id !== tab.id), tab]);
  return () => state.setTabs((prev) => prev.filter((t) => t !== tab));
}
