/**
 * Shared state of the JSON Schema feature: the schema each open file uses (the status bar
 * widget reads it), whether the feature applies to a file, and change notifications that
 * make editors validate again (settings, schema files on disk).
 */
import { createRoot, createEffect, on } from "solid-js";
import { createStore } from "solid-js/store";
import type { SchemaInfo, SchemaMapping } from "@/ipc/jsonSchema";
import { isSchemaLocation } from "@/ipc/jsonSchema";
import { languageIdFor } from "@/editor/languages";
import { servers } from "@/features/lsp/store";
import { onFsChanged } from "@/state/fsWatch";
import { isPluginActive } from "@/state/project";
import { getSetting } from "@/state/settings";

export const PLUGIN_ID = "forge.json-schema";
export const MAPPINGS_KEY = "jsonSchema.mappings";

/** Editor language ids whose files are JSON or YAML. */
export const SCHEMA_LANGUAGES = new Set(["json", "yaml", "docker-compose", "symfony-services"]);

/** Languages whose own hand-written schema already completes and documents keys. */
export const OWN_COMPLETION = new Set(["docker-compose", "symfony-services"]);

/** Language servers that validate with schemas themselves; Forge steps back while they run. */
const SERVERS: Record<string, string[]> = {
  json: ["vscode-json-language-server"],
  yaml: ["yaml-language-server"],
  "docker-compose": ["yaml-language-server"],
  "symfony-services": ["yaml-language-server"],
};

export const active = () => isPluginActive(PLUGIN_ID);

/** The language id when the file is one JSON Schema applies to. */
export function schemaLanguage(path: string): string | null {
  if (isSchemaLocation(path)) return null;
  const id = languageIdFor(path) ?? "";
  return SCHEMA_LANGUAGES.has(id) ? id : null;
}

export function serverRuns(language: string): boolean {
  const ids = SERVERS[language] ?? [];
  return servers().some((s) => ids.includes(s.id) && (s.state === "starting" || s.state === "ready" || s.state === "busy"));
}

/** `undefined`: not validated yet; `null`: no schema. */
const [schemas, setSchemas] = createStore<Record<string, SchemaInfo | null>>({});
export { schemas };

export function setSchemaInfo(path: string, info: SchemaInfo | null): void {
  setSchemas(path, info);
}

export const mappings = () => getSetting<SchemaMapping[]>(MAPPINGS_KEY, []) ?? [];

const listeners = new Set<() => void>();

/** Called when the schemas of files may have changed (settings, schema files). */
export function onSchemasChanged(cb: () => void): () => void {
  listeners.add(cb);
  return () => listeners.delete(cb);
}

if (typeof document !== "undefined") {
  createRoot(() => {
    createEffect(
      on(
        () => [
          JSON.stringify(mappings()),
          getSetting<boolean>("jsonSchema.remote.enabled", false),
          getSetting<boolean>("jsonSchema.remote.schemaStore", true),
          active(),
        ],
        () => listeners.forEach((l) => l()),
        { defer: true },
      ),
    );
  });
  // A schema file of the project changed on disk.
  onFsChanged((c) => {
    if ([...c.changed, ...c.removed].some((p) => /\.json$/i.test(p))) listeners.forEach((l) => l());
  });
}
