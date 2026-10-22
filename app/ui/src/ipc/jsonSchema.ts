/**
 * JSON Schema for JSON and YAML files (`commands/json_schema.rs`, crate `forge-jsonschema`).
 * Offsets are UTF-16 (CodeMirror positions); `language` is the editor's language id.
 */
import { invoke } from "./index";

export type SchemaSource = "user" | "inline" | "builtin" | "catalog";

/** The schema a file uses (the status bar widget). */
export interface SchemaInfo {
  name: string;
  /** Empty when a mapping turned schemas off for the file. */
  uri: string;
  source: SchemaSource;
  /** What Go to Declaration opens: a project path or `json-schema://…`. */
  location?: string;
  /** Why the schema cannot be used (download turned off or failed, file missing, …). */
  error?: string;
}

export interface SchemaProblem {
  from: number;
  to: number;
  message: string;
  severity: "warning" | "weak";
}

export interface ValidateAnswer {
  schema: SchemaInfo | null;
  problems: SchemaProblem[];
}

export interface SchemaCompletionItem {
  label: string;
  kind: "property" | "value" | "keyword" | "snippet";
  detail?: string;
  /** Markdown. */
  documentation?: string;
  /** LSP snippet syntax (`$1`, `${1:x}`). */
  insert: string;
  required: boolean;
  deprecated: boolean;
  /** Open completion again after inserting (the value has choices). */
  retrigger: boolean;
}

export interface SchemaCompletion {
  /** Start of the typed text the items are filtered by. */
  from: number;
  replaceFrom: number;
  replaceTo: number;
  items: SchemaCompletionItem[];
}

export interface SchemaHover {
  from: number;
  to: number;
  title: string;
  markdown: string;
}

export interface SchemaTarget {
  path: string;
  /** 0-based. */
  line: number;
  column: number;
  preview: string;
}

export interface SchemaFile {
  text: string;
  detail: string;
}

export interface BuiltinSchema {
  name: string;
  uri: string;
  files: string[];
}

/** A `jsonSchema.mappings` entry. */
export type SchemaMapping = {
  name: string;
  /** A URL or a project path; empty: no schema. */
  schema: string;
  /** `4`, `6`, `7`, `2019-09`, `2020-12`, or empty (automatic). */
  version: string;
  /** One file, directory (`dir/`) or pattern per line. */
  files: string;
};

export const SCHEMA_PREFIX = "json-schema://";
export const isSchemaLocation = (path: string) => path.startsWith(SCHEMA_PREFIX);

const req = (path: string, language: string, content: string) => ({ path, language, content });

export const jsonSchemaApi = {
  validate: (path: string, language: string, content: string) =>
    invoke<ValidateAnswer>("json_schema_validate", req(path, language, content)),
  complete: (path: string, language: string, content: string, offset: number) =>
    invoke<SchemaCompletion | null>("json_schema_complete", { ...req(path, language, content), offset }),
  hover: (path: string, language: string, content: string, offset: number) =>
    invoke<SchemaHover | null>("json_schema_hover", { ...req(path, language, content), offset }),
  goto: (path: string, language: string, content: string, offset: number) =>
    invoke<SchemaTarget[]>("json_schema_goto", { ...req(path, language, content), offset }),
  read: (location: string) => invoke<SchemaFile>("json_schema_read", { location }),
  builtins: () => invoke<BuiltinSchema[]>("json_schema_builtins"),
};
