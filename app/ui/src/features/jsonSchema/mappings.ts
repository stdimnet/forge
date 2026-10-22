/**
 * Editing `jsonSchema.mappings` (pure helpers): the status bar widget assigns a schema to a
 * file, the settings page edits the mappings' file lists. The matching itself happens in
 * Rust (`forge_jsonschema::mapping`).
 */
import type { SchemaMapping } from "@/ipc/jsonSchema";

export type MappingKind = "file" | "directory" | "pattern";

export const fileLines = (files: string): string[] =>
  files
    .split("\n")
    .map((f) => f.trim())
    .filter(Boolean);

/** How an entry of a mapping's file list is matched. */
export function entryKind(entry: string): MappingKind {
  if (entry.endsWith("/")) return "directory";
  if (/[*?{[]/.test(entry)) return "pattern";
  return "file";
}

/**
 * Use `schema` for `path`: the file leaves every mapping that lists it (mappings left without
 * files are removed) and joins the mapping with that name and schema, or a new one. An empty
 * schema is "No JSON schema".
 */
export function assignSchema(mappings: readonly SchemaMapping[], path: string, target: { name: string; schema: string }): SchemaMapping[] {
  const next: SchemaMapping[] = [];
  for (const m of mappings) {
    const before = fileLines(m.files);
    const after = before.filter((f) => f !== path);
    if (before.length && !after.length) continue;
    next.push(after.length === before.length ? m : { ...m, files: after.join("\n") });
  }
  const same = next.findIndex((m) => m.name === target.name && m.schema === target.schema);
  if (same >= 0) {
    next[same] = { ...next[same], files: [...fileLines(next[same].files), path].join("\n") };
  } else {
    next.push({ name: target.name, schema: target.schema, version: "", files: path });
  }
  return next;
}

/** A name for a new mapping that no other mapping has. */
export function newMappingName(mappings: readonly SchemaMapping[]): string {
  const names = new Set(mappings.map((m) => m.name));
  for (let i = 1; ; i++) {
    const name = i === 1 ? "New Schema" : `New Schema (${i})`;
    if (!names.has(name)) return name;
  }
}

export const SCHEMA_VERSIONS: { value: string; label: string }[] = [
  { value: "", label: "Automatic (from $schema)" },
  { value: "4", label: "JSON Schema version 4" },
  { value: "6", label: "JSON Schema version 6" },
  { value: "7", label: "JSON Schema version 7" },
  { value: "2019-09", label: "JSON Schema 2019-09" },
  { value: "2020-12", label: "JSON Schema 2020-12" },
];
