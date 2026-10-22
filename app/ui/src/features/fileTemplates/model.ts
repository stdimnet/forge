/**
 * File templates in the UI: the catalog (from Rust, refreshed when the templates setting
 * changes), the entries of File | New, the Project view's New submenu and the Alt+Insert
 * "New" popup, and the dialog requests they open.
 */
import { createRoot, createSignal, createEffect, on } from "solid-js";
import { templatesApi, type NewFile, type TemplateCatalog, type TemplateInfo } from "@/ipc/templates";
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

/** A submenu of the New menu: its label and its sections (separated in the menu). */
export interface NewMenuGroup {
  label: string;
  sections: TemplateInfo[][];
}

const SUBMENU_LABELS: Record<string, string> = { php: "PHP", web: "Web", lua: "Lua" };

/**
 * The New menu's file templates as submenus: PHP (Class, Interface, Trait, Enum, then the
 * other PHP files, then the project's framework templates), Web and Lua; user templates
 * stay top-level (label "").
 */
export function newMenuGroups(
  templates: TemplateInfo[],
  uses: (projectType: string) => boolean = (t) => usesProjectType(t, projectInfo()),
): NewMenuGroup[] {
  const files = templates.filter((t) => t.kind === "file" && (!t.when || uses(t.when)));
  const of = (group: string) => files.filter((t) => t.group === group);
  const php = of("php");
  const classes = php.filter((t) => t.builtin && (t.name === "PHP Class" || DIALOG_ONLY.has(t.name)));
  const groups: NewMenuGroup[] = [
    { label: "PHP", sections: [classes, php.filter((t) => !classes.includes(t)), of("framework")] },
    { label: SUBMENU_LABELS.web, sections: [of("web")] },
    { label: SUBMENU_LABELS.lua, sections: [of("lua")] },
    { label: "", sections: [of("user")] },
  ];
  return groups
    .map((g) => ({ ...g, sections: g.sections.filter((s) => s.length > 0) }))
    .filter((g) => g.sections.length > 0);
}

/** `PHP Interface` → `Interface` inside the PHP submenu; other names stay as they are. */
export const newMenuLabel = (t: TemplateInfo, submenu: string): string =>
  submenu === "PHP" && t.builtin ? t.name.replace(/^PHP (?=Class$|Interface$|Trait$|Enum$|File$)/, "") : t.name;

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

/** Create class from usage (Alt+Enter): what the Create New PHP Class dialog starts with. */
export interface CreateClassRequest {
  name: string;
  namespace: string;
  extends?: string;
  /** Runs after the file was created and opened, with the new class's FQN. */
  onCreated?: (file: NewFile, fqn: string) => void | Promise<void>;
}

/** What a New ▸ <template> action opens. */
export interface NewFileDialogRequest {
  template: TemplateInfo;
  dir: string;
  /** Opened from a usage: the dialog is "Create New PHP Class" with these values. */
  create?: CreateClassRequest;
}

const [dialog, setDialog] = createSignal<NewFileDialogRequest | null>(null);
export { dialog, setDialog };

export function openNewFileDialog(template: TemplateInfo, dir: string): void {
  setDialog({ template, dir });
}
