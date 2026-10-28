/**
 * Web tooling IPC (`app/src-tauri/src/commands/web_tooling.rs`, logic in
 * `crates/forge-runtools` (File Watchers) and `packs/webforge/web-tooling`): File Watchers,
 * OpenAPI, GraphQL, Nx, Nuxt and Deno; import aliases of `commands/web.rs`; Live Edit of the
 * Browser Companion; cURL import/export of the HTTP client. Offsets are UTF-16.
 */
import { invoke } from "./index";
import type { WebLocation } from "./web";

// ---- File Watchers ---------------------------------------------------------------------

export type ShowConsole = "always" | "error" | "never";
export type WatcherTrigger = "save" | "change" | "external";

export interface FileWatcher {
  name: string;
  enabled: boolean;
  template: string;
  fileType: string;
  scope: string;
  program: string;
  arguments: string;
  outputPaths: string;
  workingDirectory: string;
  environment: string;
  autoSave: boolean;
  onExternal: boolean;
  trackOnlyRoot: boolean;
  showConsole: ShowConsole;
  outputFilters: string[];
}

export interface WatcherTemplate {
  id: string;
  label: string;
  watcher: FileWatcher;
}

export interface WatcherRun {
  watcher: string;
  file: string;
  program: string;
  args: string[];
  cwd: string;
  env: Record<string, string>;
  outputs: string[];
  outputFilters: string[];
  showConsole: ShowConsole;
  commandLine: string;
}

export interface WatcherProblem {
  path: string;
  line?: number;
  column?: number;
  message: string;
}

/** File types of the watcher dialog (`forge_runtools::watchers::FILE_TYPES`). */
export const WATCHER_FILE_TYPES: { id: string; label: string }[] = [
  { id: "scss", label: "SCSS style sheet" },
  { id: "sass", label: "Sass style sheet" },
  { id: "less", label: "Less style sheet" },
  { id: "css", label: "Cascading style sheet" },
  { id: "typescript", label: "TypeScript" },
  { id: "tsx", label: "TypeScript JSX" },
  { id: "javascript", label: "JavaScript" },
  { id: "html", label: "HTML" },
  { id: "vue", label: "Vue.js" },
  { id: "json", label: "JSON" },
  { id: "markdown", label: "Markdown" },
  { id: "any", label: "Any" },
];

/** A watcher as the `tools.fileWatchers` settings object (string values). */
export function watcherToObject(w: FileWatcher): Record<string, string> {
  return {
    name: w.name,
    enabled: String(w.enabled),
    template: w.template,
    fileType: w.fileType,
    scope: w.scope,
    program: w.program,
    arguments: w.arguments,
    outputPaths: w.outputPaths,
    workingDirectory: w.workingDirectory,
    environment: w.environment,
    autoSave: String(w.autoSave),
    onExternal: String(w.onExternal),
    trackOnlyRoot: String(w.trackOnlyRoot),
    showConsole: w.showConsole,
    outputFilters: w.outputFilters.join("\n"),
  };
}

export function watcherFromObject(o: Record<string, unknown>): FileWatcher {
  const s = (k: string, d = "") => (o[k] === undefined || o[k] === null ? d : String(o[k]));
  const b = (k: string, d: boolean) => (o[k] === undefined ? d : s(k) === "true");
  const show = s("showConsole", "error");
  return {
    name: s("name"),
    enabled: b("enabled", true),
    template: s("template"),
    fileType: s("fileType"),
    scope: s("scope"),
    program: s("program"),
    arguments: s("arguments"),
    outputPaths: s("outputPaths"),
    workingDirectory: s("workingDirectory"),
    environment: s("environment"),
    autoSave: b("autoSave", false),
    onExternal: b("onExternal", false),
    trackOnlyRoot: b("trackOnlyRoot", false),
    showConsole: show === "always" || show === "never" ? show : "error",
    outputFilters: s("outputFilters")
      .split("\n")
      .map((l) => l.trim())
      .filter(Boolean),
  };
}

export const fileWatchersApi = {
  templates: () => invoke<WatcherTemplate[]>("file_watchers_templates"),
  validate: (watcher: FileWatcher, others: FileWatcher[]) => invoke<string[]>("file_watchers_validate", { watcher, others }),
  /** Runs for saved / changed files; refused in Safe Mode. */
  plan: (paths: string[], trigger: WatcherTrigger) => invoke<WatcherRun[]>("file_watchers_plan", { paths, trigger }),
  problems: (output: string, filters: string[], cwd: string) =>
    invoke<WatcherProblem[]>("file_watchers_problems", { output, filters, cwd }),
};

// ---- Imports and aliases -----------------------------------------------------------------

export interface ImportItem {
  label: string;
  dir: boolean;
  path: string;
}

export interface BundlerAlias {
  find: string;
  target: string;
  exact: boolean;
  source: string;
}

export const importsApi = {
  goto: (path: string, content: string, offset: number) => invoke<WebLocation | null>("web_import_goto", { path, content, offset }),
  complete: (path: string, content: string, offset: number) =>
    invoke<{ from: number; items: ImportItem[] } | null>("web_import_complete", { path, content, offset }),
  aliases: () => invoke<BundlerAlias[]>("web_aliases"),
};

// ---- OpenAPI ---------------------------------------------------------------------------

export interface OpenApiEndpoint {
  method: string;
  path: string;
  operationId?: string;
  summary?: string;
  tags: string[];
  deprecated: boolean;
  line: number;
}

export interface OpenApiFile {
  path: string;
  title: string;
  spec: string;
  endpoints: OpenApiEndpoint[];
}

export interface OpenApiParameter {
  name: string;
  location: string;
  required: boolean;
  description?: string;
  typeText: string;
}

export interface OpenApiBody {
  contentType: string;
  typeText: string;
  example: string;
}

export interface OpenApiOperation extends OpenApiEndpoint {
  description?: string;
  parameters: OpenApiParameter[];
  requestBody?: OpenApiBody;
  responses: { status: string; description: string; bodies: OpenApiBody[] }[];
}

export interface OpenApiPreview {
  spec: string;
  title: string;
  version: string;
  description?: string;
  servers: { url: string; description?: string }[];
  groups: { name: string; description?: string; operations: OpenApiOperation[] }[];
  schemas: {
    name: string;
    typeText: string;
    line: number;
    properties: { name: string; typeText: string; required: boolean; description?: string }[];
  }[];
}

export const openApiApi = {
  endpoints: () => invoke<OpenApiFile[]>("openapi_endpoints"),
  preview: (path: string, content: string) => invoke<OpenApiPreview | null>("openapi_preview", { path, content }),
  httpRequest: (path: string, content: string, method: string, opPath: string) =>
    invoke<string | null>("openapi_http_request", { path, content, method, opPath }),
};

// ---- GraphQL ---------------------------------------------------------------------------

export interface GraphqlProblem {
  from: number;
  to: number;
  message: string;
}

export interface GraphqlItem {
  label: string;
  kind: "field" | "argument" | "type" | "keyword" | "enum";
  detail?: string;
  documentation?: string;
  deprecated: boolean;
}

export interface GraphqlEndpoint {
  name: string;
  url: string;
  headers: Record<string, string>;
  schemaFile?: string;
}

export interface GraphqlInfo {
  config: { path: string; schema: string[]; documents: string[]; endpoints: GraphqlEndpoint[] } | null;
  schemaFiles: string[];
  types: string[];
}

export const graphqlApi = {
  info: () => invoke<GraphqlInfo>("graphql_info"),
  validate: (path: string, content: string) => invoke<GraphqlProblem[]>("graphql_validate", { path, content }),
  complete: (content: string, offset: number) => invoke<{ from: number; items: GraphqlItem[] }>("graphql_complete", { content, offset }),
  /** Introspect `url` and save the SDL; returns the written file. */
  introspect: (url: string, headers?: Record<string, string>, target?: string) =>
    invoke<string>("graphql_introspect", { url, headers, target }),
};

// ---- Nx, Nuxt, Deno --------------------------------------------------------------------

export interface NxTarget {
  name: string;
  executor: string;
  path: string;
  line: number;
}

export interface NxProject {
  name: string;
  root: string;
  kind: "application" | "library" | string;
  tags: string[];
  targets: NxTarget[];
  dependencies: string[];
  config: string;
}

export interface NxWorkspace {
  projects: NxProject[];
  targetDefaults: string[];
  defaultBase?: string;
}

export interface NuxtProject {
  root: string;
  routes: { route: string; name: string; path: string }[];
  components: { name: string; path: string }[];
}

export const nxApi = { workspace: () => invoke<NxWorkspace>("nx_workspace") };
export const nuxtApi = { project: (root?: string) => invoke<NuxtProject>("nuxt_project", { root }) };
export const denoApi = { tasks: () => invoke<{ name: string; command: string; line: number }[]>("deno_tasks") };

// ---- Live Edit and HTTP client extras ----------------------------------------------------

export interface LiveEditEvent {
  seq: number;
  kind: "css" | "reload";
  file: string;
  name: string;
  text?: string;
}

export const liveEditApi = {
  /** Tell paired browsers that `path` changed (`text`: unsaved CSS for a hot swap). */
  push: (path: string, text?: string) => invoke<LiveEditEvent | null>("companion_live_edit", { path, text }),
};

export const httpExtrasApi = {
  /** "Copy as cURL" for the request at `line`. */
  toCurl: (content: string, line: number, env?: string, source?: string) =>
    invoke<string>("scratches_http_curl", { content, line, env, source }),
  /** "Import from cURL": the `.http` text of a `curl …` command. */
  fromCurl: (command: string) => invoke<string>("scratches_http_from_curl", { command }),
  clearTokens: () => invoke<void>("scratches_http_clear_tokens"),
};
