/**
 * Inspections in the UI: the registry and profiles from the backend, and the Rust calls
 * (`forge-inspections` through WebAssembly, synchronously, in the desktop app and in browser
 * mode) for the Settings tree, profile edits, the editor's diagnostics filter, suppressions,
 * Run Inspection by Name and the results tree. This module only gathers inputs and renders.
 */
import { createEffect, createRoot, createSignal, on, untrack } from "solid-js";
import { forgeWasm, type ForgeWasm } from "@/wasm";
import {
  inspectionsApi,
  type Grouping,
  type InspectionSpec,
  type Profile,
  type ProfileOp,
  type ProfilesState,
  type Registry,
  type Report,
  type ResultNode,
  type SeverityDef,
  type SuppressAction,
  type ViewGroup,
} from "@/ipc/inspections";
import type { EditorDiagnostic } from "@/ipc/lsp";
import { refilterDiagnostics, setDiagnosticsFilter } from "@/features/lsp/store";
import { scopeContext, scopeSet } from "@/features/scopes/model";
import { viewFor } from "@/editor/views";

let wasm: ForgeWasm | null = null;
const [ready, setReady] = createSignal(false);
export { ready as inspectionsReady };

void forgeWasm().then(
  (w) => {
    wasm = w;
    setReady(true);
  },
  (e) => console.warn("inspections not loaded", e),
);

function call<T>(pick: (w: ForgeWasm) => (json: string) => string, input: unknown, fallback: T): T {
  if (!wasm) return fallback;
  try {
    const out = JSON.parse(pick(wasm)(JSON.stringify(input))) as T | { error: string };
    if (out && typeof out === "object" && !Array.isArray(out) && "error" in out && Object.keys(out).length === 1) {
      console.warn("inspections:", (out as { error: string }).error);
      return fallback;
    }
    return out as T;
  } catch (e) {
    console.warn("inspections:", e);
    return fallback;
  }
}

/** Like `call`, but the Rust error comes back as a message (profile operations). */
function callOrError<T>(pick: (w: ForgeWasm) => (json: string) => string, input: unknown): T | { error: string } {
  if (!wasm) return { error: "Inspections are not loaded yet" };
  try {
    return JSON.parse(pick(wasm)(JSON.stringify(input))) as T | { error: string };
  } catch (e) {
    return { error: String(e) };
  }
}

const [registry, setRegistry] = createSignal<Registry | null>(null);
const [builtinSeverities, setBuiltinSeverities] = createSignal<SeverityDef[]>([]);
const [profiles, setProfiles] = createSignal<ProfilesState | null>(null);

export { registry, profiles, setProfiles };

/** Load the registry and the profiles (project opened, plugins or profiles changed). */
export async function loadInspections(): Promise<void> {
  const r = await inspectionsApi.registry();
  setRegistry(r.registry);
  setBuiltinSeverities(r.severities);
  setProfiles(await inspectionsApi.profiles());
}

export const specOf = (id: string): InspectionSpec | undefined => registry()?.inspections.find((s) => s.id === id);

export function selectedProfile(state: ProfilesState | null = profiles()): Profile | undefined {
  if (!state) return undefined;
  return state.profiles.find((p) => p.name === state.selected) ?? state.profiles[0];
}

/** Every severity: the built-in ones, then the custom ones. */
export function severities(custom: SeverityDef[] = profiles()?.severities ?? []): SeverityDef[] {
  return call((w) => w.inspections_severities, { custom }, builtinSeverities());
}

export const severityName = (level: string | null | undefined, custom?: SeverityDef[]): string =>
  level ? (severities(custom).find((s) => s.id === level)?.name ?? level) : "As reported by the tool";

/** The Settings tree of `profile`, filtered by `query`. */
export function profileView(profile: Profile, query: string): ViewGroup | null {
  const reg = registry();
  if (!reg) return null;
  return call((w) => w.inspections_view, { registry: reg, profile, query }, null);
}

/** Apply a change to a profiles state (a copy comes back), or the reason it failed. */
export function profileOp(state: ProfilesState, op: ProfileOp): ProfilesState | { error: string } {
  const reg = registry();
  if (!reg) return { error: "No inspections" };
  return callOrError<ProfilesState>((w) => w.inspections_profile_op, { registry: reg, state, op });
}

/** Run Inspection by Name: matching inspections, best first. */
export function findInspections(query: string, limit = 60): InspectionSpec[] {
  const reg = registry();
  if (!reg) return [];
  return call((w) => w.inspections_find, { registry: reg, query, limit }, []);
}

/**
 * The Suppress actions for a problem of `id` at a position: changes in UTF-16 offsets for the
 * editor, in bytes (`bytes`) for the backend (Inspection Results).
 */
export function suppressActions(path: string, text: string, line: number, character: number, id: string, bytes = false): SuppressAction[] {
  return call((w) => w.inspections_suppress_actions, { path, text, line, character, id, bytes }, []);
}

/** The Inspection Results tree. */
export function reportTree(report: Report, levels: Grouping[]): ResultNode[] {
  return call((w) => w.inspections_report_tree, { report, levels }, []);
}

/** A profile as XML or JSON text (browser mode's export). */
export function profileText(profile: Profile, format: "xml" | "json"): string {
  const reg = registry();
  if (!reg) return "";
  return call((w) => w.inspections_export_profile, { registry: reg, profile, format }, "");
}

// ---- The editor's diagnostics go through the selected profile ---------------------------------

let configured = false;

function configure(): void {
  const reg = registry();
  const state = profiles();
  const profile = selectedProfile(state);
  if (!ready() || !reg || !state || !profile) return;
  configured = call(
    (w) => w.inspections_configure,
    {
      registry: reg,
      profile,
      severities: state.severities,
      scopes: scopeSet(),
      // Test folders matter for scope-specific severities; open files do not.
      context: { testRoots: untrack(scopeContext).testRoots ?? [] },
    },
    false,
  );
  refilterDiagnostics();
}

/** The profile's view of a source's diagnostics for `path`. */
export function filterDiagnostics(path: string, source: string, diagnostics: EditorDiagnostic[]): EditorDiagnostic[] {
  if (!configured) return diagnostics;
  const text = viewFor(path)?.state.doc.toString();
  return call((w) => w.inspections_apply, { path, source, text, diagnostics }, diagnostics);
}

if (typeof window !== "undefined") {
  createRoot(() => {
    createEffect(on([ready, registry, profiles, scopeSet], configure));
  });
  setDiagnosticsFilter(filterDiagnostics);
}
