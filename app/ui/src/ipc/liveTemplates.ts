/**
 * Live templates computed in Rust (`crates/forge-livetemplates`, `commands/live_templates.rs`):
 * the template catalog, expansion, Surround With and Unwrap/Remove. Offsets are UTF-16, like
 * CodeMirror's. `expandSync` runs the same Rust code as WebAssembly for keys that must decide
 * at once whether they expand (Tab).
 */
import { invoke } from "./index";
import { callWasmSync, forgeWasm, type ForgeWasm } from "@/wasm";

export interface TemplateVariable {
  name: string;
  expression: string;
  defaultValue: string;
  /** `false` is "Skip if defined". */
  alwaysStopAt: boolean;
}

export interface LiveTemplate {
  /** `group/abbreviation`. */
  id: string;
  group: string;
  abbreviation: string;
  description: string;
  text: string;
  variables: TemplateVariable[];
  contexts: string[];
  /** "Applicable in" summary, e.g. "PHP: statement". */
  applicable: string;
  enabled: boolean;
  /** Id of the built-in template this is or replaces. */
  builtin?: string;
  modified: boolean;
}

export interface TemplateGroup {
  name: string;
  framework?: string;
  /** Applies in the open project (framework groups need the framework). */
  active: boolean;
  builtin: boolean;
  templates: LiveTemplate[];
}

export interface ContextNode {
  id: string;
  label: string;
  children?: ContextNode[];
}

export interface TemplateCatalog {
  groups: TemplateGroup[];
  contexts: ContextNode[];
}

/** The `editor.liveTemplates.*` settings and the project's frameworks. */
export interface TemplateSettings {
  custom: Record<string, string>[];
  disabled: string[];
  frameworks: string[];
}

export interface ExpandEnv {
  nowMs: number;
  tzOffsetMinutes: number;
  clipboard: string;
  user: string;
  indentUnit: string;
}

export interface TemplateField {
  from: number;
  to: number;
  /** Stop order from 1; occurrences of one variable share it. */
  index: number;
}

/** Replace `from..to` of the document with `text`. */
export interface Expansion {
  from: number;
  to: number;
  text: string;
  fields: TemplateField[];
  end: number;
  templateId: string;
}

interface DocRequest {
  text: string;
  path: string;
  languageId?: string;
  settings: TemplateSettings;
}

export interface ExpandRequest extends DocRequest {
  offset: number;
  templateId?: string;
  env: ExpandEnv;
}

export interface ApplicableItem {
  id: string;
  abbreviation: string;
  description: string;
  group: string;
}

export interface ApplicableResult {
  prefix: string;
  items: ApplicableItem[];
}

export interface SurroundRequest extends DocRequest {
  from: number;
  to: number;
  env: ExpandEnv;
  id?: string;
  templatesOnly?: boolean;
}

export interface SurroundItem {
  id: string;
  label: string;
  mnemonic: string;
  kind: "surrounder" | "template";
  description?: string;
}

export interface UnwrapOption {
  label: string;
  from: number;
  to: number;
  keep?: [number, number];
  edits: { from: number; to: number; text: string }[];
}

let wasm: ForgeWasm | null = null;
void forgeWasm().then(
  (w) => (wasm = w),
  () => undefined,
);

export const liveTemplatesApi = {
  catalog: (settings: TemplateSettings) => invoke<TemplateCatalog>("live_templates_catalog", { settings }),
  applicable: (request: Omit<DocRequest, never> & { offset: number }) =>
    invoke<ApplicableResult>("live_templates_applicable", { request }),
  expand: (request: ExpandRequest) => invoke<Expansion | null>("live_templates_expand", { request }),
  surroundItems: (request: SurroundRequest) => invoke<SurroundItem[]>("live_templates_surround_items", { request }),
  surround: (request: SurroundRequest) => invoke<Expansion | null>("live_templates_surround", { request }),
  unwrap: (request: { text: string; offset: number; path: string; languageId?: string }) =>
    invoke<UnwrapOption[]>("live_templates_unwrap", { request }),
  variables: (text: string) => invoke<string[]>("live_templates_variables", { text }),
  user: () => invoke<string>("live_templates_user"),
  /**
   * Expansion right now, in-process; `undefined` while the WebAssembly module is still
   * loading (callers then fall back to `expand`).
   */
  expandSync(request: ExpandRequest): Expansion | null | undefined {
    if (!wasm) return undefined;
    return callWasmSync<Expansion | null>(wasm.live_templates_expand, request);
  },
};
