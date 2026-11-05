/**
 * Keeps the editor tabs of the LightEdit window and the Rust tab model
 * (`forge_window::light_edit`) in step: tabs Rust added (a later "Open With → Forge") open
 * here, tabs closed here are closed there. Pure over the two id lists, so it is tested
 * without a window.
 */
import type { LightEditSnapshot } from "@/ipc/lightEdit";
import { LIGHT_EDIT_PREFIX } from "@/ipc/lightEdit";

export interface TabSync {
  /** Files of the snapshot that have no tab yet, in order. */
  toOpen: string[];
  /** The tab to show: the file the last request asked for, else Rust's active tab. */
  show: { id: string; line: number | null; column: number | null } | null;
}

/** What to do with a snapshot, given the LightEdit tabs open in the editor. */
export function planSync(snapshot: LightEditSnapshot, openTabs: readonly string[]): TabSync {
  const open = new Set(openTabs);
  const toOpen = snapshot.files.map((f) => f.id).filter((id) => !open.has(id));
  const reveal = snapshot.reveal;
  const show = reveal ?? (snapshot.active ? { id: snapshot.active, line: null, column: null } : null);
  return { toOpen, show };
}

/** LightEdit tabs that were closed in the editor since `before` (Rust forgets them). */
export function closedTabs(before: readonly string[], now: readonly string[]): string[] {
  const still = new Set(now);
  return before.filter((id) => !still.has(id));
}

export const isLightEditTab = (path: string): boolean => path.startsWith(LIGHT_EDIT_PREFIX);
