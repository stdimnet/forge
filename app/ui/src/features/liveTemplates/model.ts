/**
 * Live templates in the UI: the settings as Rust expects them, the request environment and
 * the conversion of an expansion (text + tab stops, computed in Rust) into a CodeMirror
 * snippet. Settings entries for user templates and changed built-ins are built here from the
 * settings page form.
 */
import type { EditorView } from "@codemirror/view";
import { indentUnit } from "@codemirror/language";
import { getSetting } from "@/state/settings";
import { isPluginActive, usesFramework } from "@/state/project";
import { languageIdFor } from "@/editor/languages";
import {
  liveTemplatesApi,
  type ExpandEnv,
  type Expansion,
  type LiveTemplate,
  type TemplateSettings,
  type TemplateVariable,
} from "@/ipc/liveTemplates";

export const CUSTOM_KEY = "editor.liveTemplates.custom";
export const DISABLED_KEY = "editor.liveTemplates.disabled";
export const EXPAND_KEY = "editor.liveTemplates.expandKey";
export const PAGE_ID = "editor.liveTemplates";

export type CustomEntry = Record<string, string>;

/** Framework template sets active in the open project. */
export function activeFrameworks(): string[] {
  const out: string[] = [];
  if (usesFramework("laravel") && isPluginActive("forge.laravel")) out.push("laravel");
  if (usesFramework("symfony") && isPluginActive("forge.symfony")) out.push("symfony");
  return out;
}

/** The effective settings (or the given values, e.g. the Settings dialog draft). */
export function templateSettings(values?: { custom?: CustomEntry[]; disabled?: string[] }): TemplateSettings {
  return {
    custom: values?.custom ?? getSetting<CustomEntry[]>(CUSTOM_KEY, []) ?? [],
    disabled: values?.disabled ?? getSetting<string[]>(DISABLED_KEY, []) ?? [],
    frameworks: activeFrameworks(),
  };
}

let user = "";
let clipboard = "";
void liveTemplatesApi.user().then(
  (u) => (user = u),
  () => undefined,
);
if (typeof document !== "undefined") {
  // `clipboard()`: what was last copied or cut in Forge (reading the system clipboard would
  // need a permission prompt on every expansion).
  const remember = () => {
    const text = document.getSelection()?.toString();
    if (text) clipboard = text;
  };
  document.addEventListener("copy", remember, true);
  document.addEventListener("cut", remember, true);
}

/** Remember text Forge put on the clipboard itself (e.g. "Copy Reference"). */
export function rememberClipboard(text: string): void {
  clipboard = text;
}

export function envFor(view: EditorView): ExpandEnv {
  return {
    nowMs: Date.now(),
    tzOffsetMinutes: -new Date().getTimezoneOffset(),
    clipboard,
    user,
    indentUnit: view.state.facet(indentUnit),
  };
}

/** Document, path and language for a request about `view`. */
export function docRequest(path: string, view: EditorView) {
  return {
    text: view.state.doc.toString(),
    path,
    // Database consoles hold SQL (their file type is unknown).
    languageId: languageIdFor(path) ?? (path.startsWith("db-console://") ? "sql" : undefined),
    settings: templateSettings(),
  };
}

const escapeSnippet = (s: string) => s.replace(/[{}]/g, (c) => `\\${c}`);

/**
 * CodeMirror snippet template of an expansion: `${n:value}` for each stop (occurrences of a
 * variable share `n` and are edited together), `${0}` for the end, braces escaped.
 */
export function toSnippet(e: Expansion): string {
  type Mark = { from: number; to: number; index: number };
  const marks: Mark[] = [...e.fields.map((f) => ({ ...f })), { from: e.end, to: e.end, index: 0 }];
  marks.sort((a, b) => a.from - b.from || (a.index === 0 ? 1 : b.index === 0 ? -1 : 0));
  let out = "";
  let pos = 0;
  for (const m of marks) {
    if (m.from < pos) continue;
    out += escapeSnippet(e.text.slice(pos, m.from));
    const value = e.text.slice(m.from, m.to);
    if (m.index === 0) out += "${0}";
    // Field defaults cannot hold braces or line breaks: keep such values as plain text.
    else if (/[{}\n]/.test(value)) out += `\${${m.index}:}${escapeSnippet(value)}`;
    else out += `\${${m.index}:${value}}`;
    pos = m.to;
  }
  return out + escapeSnippet(e.text.slice(pos));
}

/** A settings entry for a template as edited on the settings page. */
export function entryFor(t: Pick<LiveTemplate, "group" | "abbreviation" | "description" | "text" | "contexts" | "variables" | "builtin" | "key">): CustomEntry {
  const entry: CustomEntry = {
    group: t.group,
    abbreviation: t.abbreviation,
    description: t.description,
    text: t.text,
    contexts: t.contexts.join(","),
  };
  if (t.variables.length) entry.variables = JSON.stringify(t.variables);
  if (t.builtin) entry.builtin = t.builtin;
  if (t.key) entry.key = t.key;
  return entry;
}

const entryId = (e: CustomEntry) => `${e.group?.trim() || "user"}/${e.abbreviation?.trim() ?? ""}`;

type TemplateIdentity = Pick<LiveTemplate, "id" | "builtin" | "key">;

/** Identity of a template on the settings page that survives renaming. */
export const identityOf = (t: TemplateIdentity): string => t.builtin ?? (t.key ? `key:${t.key}` : t.id);

/** Entries that describe `t` (its replacement of a built-in, or the user template itself). */
function describes(e: CustomEntry, t: TemplateIdentity): boolean {
  if (t.builtin) return e.builtin === t.builtin || (!e.builtin && entryId(e) === t.builtin);
  if (t.key) return e.key === t.key;
  return !e.builtin && !e.key && entryId(e) === t.id;
}

/** `custom` with `t` replaced by `next` (or removed when `next` is null). */
export function replaceEntry(custom: CustomEntry[], t: TemplateIdentity, next: CustomEntry | null): CustomEntry[] {
  const rest = custom.filter((e) => !describes(e, t));
  return next ? [...rest, next] : rest;
}

/** `custom` after deleting `t`: built-ins are marked removed, user templates dropped. */
export function removeEntry(custom: CustomEntry[], t: LiveTemplate): CustomEntry[] {
  const rest = custom.filter((e) => !describes(e, t));
  return t.builtin ? [...rest, { builtin: t.builtin, group: t.group, abbreviation: t.abbreviation, removed: "true" }] : rest;
}

/** `custom` with a built-in template back to its shipped state. */
export const restoreEntry = (custom: CustomEntry[], t: LiveTemplate): CustomEntry[] => custom.filter((e) => !describes(e, t));

/** Variables of `text` in order, keeping the settings of those that already exist. */
export function mergeVariables(names: string[], current: TemplateVariable[]): TemplateVariable[] {
  return names.map((name) => current.find((v) => v.name === name) ?? { name, expression: "", defaultValue: "", alwaysStopAt: true });
}

/** Contexts after toggling `id` in the "Applicable in" tree (`children` of its parent). */
export function toggleContext(contexts: string[], id: string, parent: { id: string; children: string[] } | null, on: boolean): string[] {
  const set = new Set(contexts);
  if (!parent) {
    // A parent: on covers its children (comments and strings stay separate, as in PHPStorm).
    set.delete(id);
    if (on) set.add(id);
    return [...set];
  }
  if (set.has(parent.id) && !on) {
    set.delete(parent.id);
    for (const c of parent.children) if (c !== id && !/_(COMMENT|STRING)$/.test(c)) set.add(c);
  } else if (on) set.add(id);
  else set.delete(id);
  return [...set];
}
