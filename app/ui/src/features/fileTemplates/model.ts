/**
 * File templates in the UI: the catalog (from Rust, refreshed when the templates setting
 * changes), the entries of File | New, the Project view's New submenu and the Alt+Insert
 * "New" popup, and the dialog requests they open.
 */
import { createRoot, createSignal, createEffect, on } from "solid-js";
import { templatesApi, type TemplateCatalog, type TemplateInfo } from "@/ipc/templates";
import { getSetting, settingsState } from "@/state/settings";
import { workspace } from "@/state/workspace";
import { projectInfo, usesProjectType } from "@/state/project";

export const CUSTOM_KEY = "fileTemplates.custom";
export const SETTINGS_PAGE = "editor.file-templates";

const EMPTY: TemplateCatalog = { templates: [], variables: [] };
const [catalog, setCatalog] = createSignal<TemplateCatalog>(EMPTY);
export { catalog };

/** Load the catalog now (tests, first use) and again whenever the templates setting changes. */
export async function refreshCatalog(): Promise<void> {
  try {
    setCatalog(await templatesApi.list());
  } catch (e) {
    console.warn("file templates unavailable", e);
  }
}

let tracking = false;
export function trackCatalog(): void {
  if (tracking) return;
  tracking = true;
  createRoot(() => {
    createEffect(
      on(
        () => [settingsState.loaded, JSON.stringify(getSetting(CUSTOM_KEY) ?? null), workspace()?.root] as const,
        ([loaded]) => {
          if (loaded) void refreshCatalog();
        },
      ),
    );
  });
}

/**
 * Built-ins PHPStorm reaches through the New PHP Class dialog's Template list instead of
 * their own menu entries.
 */
const DIALOG_ONLY = new Set(["PHP Interface", "PHP Trait", "PHP Enum"]);

const GROUP_ORDER = ["php", "framework", "web", "lua", "user"];

/** File templates offered for the open project, grouped (`php`, `framework`, `web`, …). */
export function offeredTemplates(
  templates: TemplateInfo[],
  uses: (projectType: string) => boolean = (t) => usesProjectType(t, projectInfo()),
): TemplateInfo[][] {
  const files = templates.filter((t) => t.kind === "file" && (!t.when || uses(t.when)));
  return GROUP_ORDER.map((g) => files.filter((t) => t.group === g && !(t.builtin && DIALOG_ONLY.has(t.name)))).filter(
    (g) => g.length > 0,
  );
}

/** Class-like PHP templates the New PHP Class dialog offers in its Template list. */
export function phpClassTemplates(templates: TemplateInfo[], current: TemplateInfo, uses?: (t: string) => boolean): TemplateInfo[] {
  const offered = offeredTemplates(templates, uses).flat();
  const basics = templates.filter((t) => t.kind === "file" && t.builtin && (t.name === "PHP Class" || DIALOG_ONLY.has(t.name)));
  const list = [...basics, ...offered.filter((t) => t.phpClass && !basics.includes(t))];
  return list.includes(current) ? list : [...list, current];
}

/** `PHP Interface` → `Interface`, like the Template list of PHPStorm's dialog. */
export const templateLabel = (t: TemplateInfo): string => (t.builtin ? t.name.replace(/^PHP /, "") : t.name);

/** `BACKED_TYPE` → `Backed type`. */
export function variableLabel(name: string): string {
  const words = name.toLowerCase().split("_").filter(Boolean).join(" ");
  return words.charAt(0).toUpperCase() + words.slice(1);
}

/** A PHP identifier (class name, namespace segment). */
export const isPhpIdentifier = (s: string): boolean => /^[\p{L}_][\p{L}\p{N}_]*$/u.test(s);

export function validateClassName(name: string): string | null {
  if (!name.trim()) return null;
  return isPhpIdentifier(name.trim()) ? null : `'${name.trim()}' is not a valid class name`;
}

export function validateNamespace(ns: string): string | null {
  const n = ns.trim().replace(/^\\+|\\+$/g, "");
  if (!n) return null;
  return n.split("\\").every(isPhpIdentifier) ? null : `'${ns.trim()}' is not a valid namespace`;
}

/** What a New ▸ <template> action opens. */
export interface NewFileDialogRequest {
  template: TemplateInfo;
  dir: string;
}

const [dialog, setDialog] = createSignal<NewFileDialogRequest | null>(null);
export { dialog, setDialog };

export function openNewFileDialog(template: TemplateInfo, dir: string): void {
  setDialog({ template, dir });
}
