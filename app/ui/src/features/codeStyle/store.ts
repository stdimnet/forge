/**
 * The Code Style pages' draft: the schemes as edited in the Settings dialog. Apply stores
 * them in Rust (`code_style_save`); Cancel throws the draft away. The scheme selected in the
 * scheme box is the one the project uses and the one every page edits.
 */
import { createStore, produce, reconcile } from "solid-js/store";
import {
  codeStyleApi,
  type CodeStyleLanguage,
  type CodeStyleModel,
  type CodeStyleScheme,
  type CodeStyleSchemes,
  type CodeStyleValue,
  type ImportedScheme,
} from "@/ipc/codeStyle";

interface State {
  model: CodeStyleModel | null;
  draft: CodeStyleSchemes | null;
  original: CodeStyleSchemes | null;
  error: string | null;
}

const [cs, setCs] = createStore<State>({ model: null, draft: null, original: null, error: null });

export { cs as codeStyle };

export const GENERAL_PAGE = "editor.code-style";
export const pageOf = (language: string) => (language === "general" ? GENERAL_PAGE : `${GENERAL_PAGE}.${language}`);

const clone = <T>(v: T): T => JSON.parse(JSON.stringify(v)) as T;

let modelLoad: Promise<CodeStyleModel> | null = null;

/** The options of every page (loaded once). */
export function loadModel(): Promise<CodeStyleModel> {
  modelLoad ??= codeStyleApi.model().then((m) => {
    setCs("model", m);
    return m;
  });
  return modelLoad;
}

/** Load the schemes unless there are unapplied edits. */
export async function loadSchemes(): Promise<void> {
  await loadModel();
  if (hasChanges()) return;
  const schemes = await codeStyleApi.schemes();
  setCs({ original: schemes, draft: clone(schemes), error: null });
}

export const languageSpec = (id: string): CodeStyleLanguage | undefined =>
  id === "general" ? (cs.model?.general ?? undefined) : cs.model?.languages.find((l) => l.id === id);

export const current = (): CodeStyleScheme | undefined =>
  cs.draft?.schemes.find((s) => s.id === cs.draft!.selected);

export const valueOf = (language: string, key: string): CodeStyleValue | undefined =>
  current()?.values[`${language}.${key}`];

function updateCurrent(fn: (s: CodeStyleScheme) => void): void {
  if (!cs.draft) return;
  const i = cs.draft.schemes.findIndex((s) => s.id === cs.draft!.selected);
  if (i >= 0) setCs("draft", "schemes", i, produce(fn));
}

export function setValue(language: string, key: string, value: CodeStyleValue): void {
  updateCurrent((s) => (s.values[`${language}.${key}`] = value));
}

export function selectScheme(id: string): void {
  if (cs.draft) setCs("draft", "selected", id);
}

/** "Set from…": the language's defaults, then the preset's values. */
export function applyPreset(language: string, presetId: string): void {
  const lang = languageSpec(language);
  const preset = lang?.presets.find((p) => p.id === presetId);
  if (!lang || !preset) return;
  updateCurrent((s) => {
    for (const o of lang.options) s.values[`${language}.${o.key}`] = o.default;
    for (const [k, v] of Object.entries(preset.values)) s.values[`${language}.${k}`] = v;
  });
}

let counter = 0;
const newId = () => `s${Date.now().toString(36)}${(counter++).toString(36)}`;

/** A free name based on `base` ("Default copy", "Default copy 2"). */
export function freeName(base: string): string {
  const taken = new Set(cs.draft?.schemes.filter((s) => s.level === "ide").map((s) => s.name.toLowerCase()) ?? []);
  if (!taken.has(base.toLowerCase())) return base;
  for (let n = 2; ; n++) if (!taken.has(`${base} ${n}`.toLowerCase())) return `${base} ${n}`;
}

/** A problem with `name` for an IDE scheme, or null. */
export function nameProblem(name: string, except?: string): string | null {
  const n = name.trim();
  if (!n) return "Enter a name";
  const clash = cs.draft?.schemes.some((s) => s.level === "ide" && s.id !== except && s.name.toLowerCase() === n.toLowerCase());
  return clash ? `A scheme named "${n}" already exists` : null;
}

function addIdeScheme(name: string, values: CodeStyleScheme["values"]): void {
  if (!cs.draft) return;
  const id = newId();
  setCs(
    "draft",
    produce((d) => {
      if (!d) return;
      // IDE schemes come before the Project scheme, like the scheme box lists them.
      const at = d.schemes.findIndex((s) => s.level === "project");
      const scheme: CodeStyleScheme = { id, name: name.trim(), level: "ide", values: clone(values) };
      if (at >= 0) d.schemes.splice(at, 0, scheme);
      else d.schemes.push(scheme);
      d.selected = id;
    }),
  );
}

/** Copy to Project: the Project scheme gets the current scheme's values and is selected. */
export function copyToProject(): void {
  const from = current();
  if (!from || !cs.draft) return;
  const i = cs.draft.schemes.findIndex((s) => s.level === "project");
  if (i < 0) return;
  setCs("draft", "schemes", i, "values", reconcile(clone(from.values)));
  setCs("draft", "selected", cs.draft.schemes[i].id);
}

/** Copy to IDE / Duplicate: a new IDE scheme with the current values. */
export function copyAs(name: string): void {
  const from = current();
  if (from) addIdeScheme(name, from.values);
}

export function renameCurrent(name: string): void {
  updateCurrent((s) => (s.name = name.trim()));
}

/** Reset: the built-in defaults (the Project scheme: the Default scheme's values). */
export function resetCurrent(): void {
  const model = cs.model;
  const scheme = current();
  if (!model || !scheme || !cs.draft) return;
  const defaults: CodeStyleScheme["values"] = {};
  if (scheme.level === "project") {
    Object.assign(defaults, cs.draft.schemes.find((s) => s.id === "default")?.values ?? {});
  } else {
    for (const lang of [model.general, ...model.languages]) {
      for (const o of lang.options) defaults[`${lang.id}.${o.key}`] = o.default;
    }
  }
  updateCurrent((s) => (s.values = clone(defaults)));
}

export function deleteCurrent(): void {
  const scheme = current();
  if (!scheme || scheme.id === "default" || scheme.level === "project") return;
  setCs(
    "draft",
    produce((d) => {
      if (!d) return;
      d.schemes = d.schemes.filter((s) => s.id !== scheme.id);
      d.selected = "default";
    }),
  );
}

/** Import into the current scheme or as a new IDE scheme. */
export function importScheme(imported: ImportedScheme, intoCurrent: boolean): void {
  if (intoCurrent) updateCurrent((s) => (s.values = clone(imported.values)));
  else addIdeScheme(freeName(imported.name), imported.values);
}

// ---- Draft state for the Settings dialog ---------------------------------------------------

const prefixed = (values: CodeStyleScheme["values"], language: string) =>
  JSON.stringify(Object.entries(values).filter(([k]) => k.startsWith(`${language}.`)).sort());

function languageModified(language: string): boolean {
  const { draft, original } = cs;
  if (!draft || !original) return false;
  return draft.schemes.some((s) => {
    const before = original.schemes.find((o) => o.id === s.id);
    return !before || prefixed(before.values, language) !== prefixed(s.values, language);
  });
}

const schemeList = (s: CodeStyleSchemes) => JSON.stringify([s.selected, s.schemes.map((x) => [x.id, x.name])]);

export function pageModified(pageId: string): boolean {
  const { draft, original } = cs;
  if (!draft || !original) return false;
  if (pageId === GENERAL_PAGE) return schemeList(draft) !== schemeList(original) || languageModified("general");
  if (!pageId.startsWith(`${GENERAL_PAGE}.`)) return false;
  return languageModified(pageId.slice(GENERAL_PAGE.length + 1));
}

export const hasChanges = (): boolean =>
  !!cs.draft && !!cs.original && JSON.stringify(cs.draft) !== JSON.stringify(cs.original);

export async function applyDraft(): Promise<void> {
  if (!cs.draft) return;
  const saved = await codeStyleApi.save(clone(cs.draft));
  setCs({ original: saved, draft: clone(saved) });
}

export function discardDraft(): void {
  if (cs.original) setCs("draft", reconcile(clone(cs.original)));
}

/** "Reset" of one page: its values (and on the General page the scheme list) as applied. */
export function resetPage(pageId: string): void {
  const { draft, original } = cs;
  if (!draft || !original) return;
  if (pageId === GENERAL_PAGE) {
    // Scheme list, selection and General values as they were; other pages keep their edits.
    const edited = clone(draft);
    const restored = clone(original);
    for (const s of restored.schemes) {
      const e = edited.schemes.find((x) => x.id === s.id);
      if (!e) continue;
      for (const [k, v] of Object.entries(e.values)) if (!k.startsWith("general.")) s.values[k] = v;
    }
    setCs("draft", reconcile(restored));
    return;
  }
  const language = pageId.slice(GENERAL_PAGE.length + 1);
  setCs(
    "draft",
    produce((d) => {
      if (!d) return;
      for (const s of d.schemes) {
        const before = original.schemes.find((o) => o.id === s.id);
        if (!before) continue;
        for (const k of Object.keys(s.values)) if (k.startsWith(`${language}.`)) s.values[k] = before.values[k];
      }
    }),
  );
}

/** Test hook: set the store directly. */
export function setCodeStyleState(state: Partial<State>): void {
  setCs(state);
}
