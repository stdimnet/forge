/**
 * The Inspections page's unapplied edits (Settings | Editor | Inspections): a copy of the
 * profiles that every change goes through Rust (`ProfileOp`), stored on Apply / OK.
 */
import { createSignal } from "solid-js";
import { inspectionsApi, type ProfileOp, type ProfilesState } from "@/ipc/inspections";
import { showBalloon } from "@/keymap";
import { loadInspections, profileOp, profiles, setProfiles } from "./model";

export const INSPECTIONS_PAGE = "editor.inspections";

const [draft, setDraft] = createSignal<ProfilesState | null>(null);
export { draft };

/** Start editing (the page opened): a copy of the stored profiles. */
export async function beginDraft(): Promise<void> {
  if (draft()) return;
  if (!profiles()) await loadInspections().catch((e) => console.warn("inspections could not be loaded", e));
  const p = profiles();
  if (p) setDraft(structuredClone(p));
}

/** Change the draft; a failed change is shown as a balloon and returns false. */
export function change(op: ProfileOp): boolean {
  const d = draft();
  if (!d) return false;
  const next = profileOp(d, op);
  if ("error" in next) {
    showBalloon("Inspections", next.error, 4000, undefined, { type: "warning" });
    return false;
  }
  setDraft(next);
  return true;
}

export const hasChanges = (): boolean => {
  const d = draft();
  return !!d && JSON.stringify(d) !== JSON.stringify(profiles());
};

export async function applyDraft(): Promise<void> {
  const d = draft();
  if (!d || !hasChanges()) return;
  const saved = await inspectionsApi.save(d);
  setProfiles(saved);
  setDraft(structuredClone(saved));
}

export function discardDraft(): void {
  setDraft(null);
}

export function resetPage(): void {
  const p = profiles();
  setDraft(p ? structuredClone(p) : null);
}
