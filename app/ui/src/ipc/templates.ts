/**
 * File and code templates (`forge-file-templates`, `commands/templates.rs`): the catalog for
 * Settings | Editor | File and Code Templates and the New menus, PSR-4 namespace ↔ folder
 * mapping for the New PHP Class dialog, and File | New ▸ <template>.
 */
import { invoke } from "./index";

export type TemplateKind = "file" | "include" | "code";

/** A user template or the changed text of a built-in (entry of `fileTemplates.custom`). */
export interface CustomTemplate {
  kind: TemplateKind;
  name: string;
  extension?: string;
  text?: string;
}

export interface TemplateInfo {
  kind: TemplateKind;
  name: string;
  extension: string;
  text: string;
  builtin: boolean;
  modified: boolean;
  defaultText?: string;
  defaultExtension?: string;
  /** Offered only when the project uses this language or framework. */
  when?: string;
  /** `php`, `framework`, `web`, `lua` or `user`. */
  group: string;
  /** New opens the New PHP Class dialog. */
  phpClass: boolean;
  /** Variables beyond the predefined ones that the New dialog asks for. */
  variables: string[];
  /** Every variable the template uses. */
  references: string[];
  description?: string;
  /** Syntax error (`Line 3: Missing #end for #if`). */
  error?: string;
}

export interface VariableInfo {
  name: string;
  description: string;
}

export interface TemplateCatalog {
  templates: TemplateInfo[];
  variables: VariableInfo[];
}

export interface DateParts {
  year: number;
  month: number;
  day: number;
  hour: number;
  minute: number;
  second: number;
  /** 0 = Sunday. */
  weekday: number;
}

export interface NewFileRequest {
  template: string;
  dir: string;
  name: string;
  fileName?: string;
  namespace?: string;
  extends?: string;
  implements?: string;
  variables?: Record<string, string>;
  context?: { projectName?: string; user?: string; now?: DateParts };
}

export interface NewFile {
  path: string;
  text: string;
}

/** The local date and time for `${DATE}`, `${TIME}`, …. */
export function nowParts(d = new Date()): DateParts {
  return {
    year: d.getFullYear(),
    month: d.getMonth() + 1,
    day: d.getDate(),
    hour: d.getHours(),
    minute: d.getMinutes(),
    second: d.getSeconds(),
    weekday: d.getDay(),
  };
}

export const templatesApi = {
  /** With `custom`, the catalog for these user templates instead of the saved ones. */
  list: (custom?: CustomTemplate[]) => invoke<TemplateCatalog>("templates_list", { custom: custom ?? null }),
  validate: (text: string) => invoke<string | null>("templates_validate", { text }),
  phpNamespace: (dir: string) => invoke<string | null>("templates_php_namespace", { dir }),
  phpDirectory: (namespace: string) => invoke<string | null>("templates_php_directory", { namespace }),
  create: (request: NewFileRequest) =>
    invoke<NewFile>("templates_create", { request: { ...request, context: { now: nowParts(), ...request.context } } }),
};
