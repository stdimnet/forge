/**
 * Symfony service configuration (`config/services*.yaml`, `config/services/*.yaml`): YAML
 * with highlighting of `@service`, `%parameter%`, `%env(…)%` and `!tags`, completion and
 * hover documentation for service keys, and completion of service ids, classes, parameters,
 * environment variables and container tags (from this file, the other files in `config/`,
 * the `.env` files and the PHP index).
 */
import { yaml } from "@codemirror/lang-yaml";
import type { Extension } from "@codemirror/state";
import { Decoration, MatchDecorator, ViewPlugin, type DecorationSet, type EditorView, type ViewUpdate } from "@codemirror/view";
import type { Completion, CompletionContext, CompletionResult } from "@codemirror/autocomplete";
import { registerLanguage } from "@/editor/languages";
import { workspaceApi } from "@/ipc/workspace";
import { phpApi } from "@/features/php/api";
import { parseEnv } from "./env";
import { flag, key, map, positionAt, schemaCompletion, schemaHover, seq, type Position, type SchemaNode } from "./yamlSchema";
import "./symfony.css";


/* ---------- Schema ---------- */

const tag = map("A container tag. `name` is required; other attributes depend on the tag.", {
  name: key("The tag name, e.g. `kernel.event_listener` or `console.command`."),
  priority: key("Order in which tagged services are processed (higher first).", "int"),
  event: key("Event name for `kernel.event_listener`."),
  method: key("Method called for `kernel.event_listener`."),
  command: key("Command name for `console.command`."),
  alias: key("Alias the tagged service can be referenced by."),
  key: key("Key of the service in an indexed tagged collection."),
});

const serviceKeys: Record<string, SchemaNode> = {
  class: key("The service class (FQCN). Optional when the service id is the class name."),
  arguments: key("Constructor arguments, positional (list) or named (`$name:` map). Reference services with `@id`, parameters with `%name%`.", "list | map"),
  calls: seq("Methods called after instantiation: `- [method, [arguments]]` or `- method: [arguments]`."),
  tags: seq("Container tags, as `name` strings or maps with `name` and attributes.", tag),
  factory: key("Factory creating the service: `[class_or_@service, method]` or `Class::method`.", "string | list"),
  constructor: key("Static factory method of the service class."),
  autowire: flag("Let the container inject constructor arguments by type."),
  autoconfigure: flag("Automatically register tags and calls from the class's interfaces and attributes."),
  public: flag("Whether the service can be fetched from the container (`$container->get()`)."),
  shared: flag("Whether the same instance is returned every time (default `true`)."),
  lazy: key("Create the service as a lazy proxy; a class or interface name restricts the proxy.", "bool | string"),
  abstract: flag("Mark the definition as a parent for other services; it is never instantiated."),
  parent: key("Id of the parent service whose definition this one inherits."),
  decorates: key("Id of the service this one decorates."),
  decoration_inner_name: key("Id under which the decorated service is available inside the decorator (default `.inner`)."),
  decoration_priority: key("Priority among several decorators of the same service.", "int"),
  decoration_on_invalid: key("What to do when the decorated service does not exist.", "string", { values: ["exception", "ignore", "null"] }),
  configurator: key("Callable that configures the service after creation: `[@service, method]` or `Class::method`.", "string | list"),
  properties: key("Public properties to set on the service.", "map"),
  bind: key("Values for arguments by name (`$name`) or type, applied to all services in scope.", "map"),
  resource: key("Glob pattern of PHP files to register as services; the service id is the namespace in the key plus the class name."),
  exclude: key("Glob pattern (or list) of files to exclude from `resource`.", "string | list"),
  namespace: key("Namespace prefix for `resource`."),
  alias: key("Id of the service this id is an alias of."),
  deprecated: key("Mark the service as deprecated: a map with `package`, `version` and `message`.", "map", {
    keys: { package: key("The vendor/package that deprecates it."), version: key("Version since when it is deprecated."), message: key("Message, `%service_id%` is replaced.") },
  }),
  synthetic: flag("The service is set at runtime, not built by the container."),
  file: key("A file to require before the service is instantiated."),
  from_callable: key("Create the service from a first-class callable: `[@service, method]` or `Class::method`.", "string | list"),
  stack: seq("Chain of decorators created in one step; each item is a service definition, `@id` or `!service`."),
};

const defaultsKeys: Record<string, SchemaNode> = {
  autowire: serviceKeys.autowire,
  autoconfigure: serviceKeys.autoconfigure,
  public: serviceKeys.public,
  bind: serviceKeys.bind,
  tags: serviceKeys.tags,
};

const service: SchemaNode = { type: "map", doc: "A service definition.", keys: serviceKeys };

export const symfonySchema: SchemaNode = {
  type: "map",
  keys: {
    parameters: map("Container parameters, referenced as `%name%`.", {}, { any: key("The parameter value.", "any") }),
    imports: seq("Other config files to import.", map("An import.", {
      resource: key("Path of the file or glob to import."),
      type: key("Loader type, e.g. `glob` or `yaml`."),
      ignore_errors: key("Ignore a missing resource (`not_found`) or all errors (`true`).", "bool | string"),
    })),
    services: map("Service definitions. Keys are service ids, or namespace prefixes ending in `\\` when combined with `resource`.", {
      _defaults: map("Defaults applied to every service in this file.", defaultsKeys),
      _instanceof: map("Configuration applied to all services implementing an interface or extending a class.", {}, { any: service }),
    }, { any: service }),
  },
};

/* ---------- Data ---------- */

export const KERNEL_PARAMETERS = [
  "kernel.project_dir", "kernel.environment", "kernel.runtime_environment", "kernel.debug", "kernel.cache_dir",
  "kernel.build_dir", "kernel.logs_dir", "kernel.charset", "kernel.container_class", "kernel.default_locale",
  "kernel.secret", "kernel.bundles", "kernel.bundles_metadata", "kernel.http_method_override", "kernel.trusted_hosts",
  "kernel.enabled_locales", "kernel.error_controller",
];

export const ENV_PROCESSORS = [
  "string", "int", "bool", "float", "json", "base64", "resolve", "file", "default", "trim", "url", "query_string",
  "csv", "enum", "key", "not", "require", "const", "shuffle", "urlencode", "direct_url", "random",
];

export const TAG_NAMES = [
  "kernel.event_listener", "kernel.event_subscriber", "kernel.cache_warmer", "kernel.cache_clearer", "kernel.reset",
  "console.command", "twig.extension", "twig.runtime", "twig.component", "doctrine.event_listener", "doctrine.event_subscriber",
  "doctrine.orm.entity_listener", "doctrine.repository_service", "validator.constraint_validator", "validator.initializer",
  "form.type", "form.type_extension", "form.type_guesser", "messenger.message_handler", "messenger.transport_factory",
  "monolog.logger", "monolog.processor", "controller.service_arguments", "controller.argument_value_resolver",
  "security.voter", "security.authenticator", "routing.loader", "routing.route_loader", "serializer.normalizer",
  "serializer.encoder", "container.service_locator", "container.service_subscriber", "container.hot_path",
  "container.no_preload", "http_client.client", "mailer.transport_factory", "translation.loader", "data_collector",
  "assets.package", "cache.pool", "property_info.type_extractor", "scheduler.task", "kernel.locale_aware",
];

/* ---------- Pure helpers ---------- */

/** Keys written directly under a top-level `section:` of a YAML document. */
export function childKeys(text: string, section: string): string[] {
  const lines = text.split("\n");
  const start = lines.findIndex((l) => new RegExp(`^${section}:\\s*(?:#.*)?$`).test(l));
  if (start < 0) return [];
  const names: string[] = [];
  let indent = -1;
  for (const line of lines.slice(start + 1)) {
    if (!line.trim() || /^\s*#/.test(line)) continue;
    const lead = line.length - line.trimStart().length;
    if (lead === 0) break;
    if (indent < 0) indent = lead;
    if (lead !== indent) continue;
    const m = /^\s+(?:"([^"]+)"|'([^']+)'|([^\s:#'"-][^:#]*?|-[^\s:#][^:#]*?))\s*:(?:\s|$)/.exec(line);
    const name = m && (m[1] ?? m[2] ?? m[3]);
    if (name) names.push(name);
  }
  return names;
}

/** Service ids: keys under `services:` except `_defaults`, `_instanceof` and namespace prefixes. */
export function serviceIds(text: string): string[] {
  return childKeys(text, "services").filter((k) => k !== "_defaults" && k !== "_instanceof" && !k.endsWith("\\") && !k.endsWith("\\\\"));
}

export const parameterNames = (text: string): string[] => childKeys(text, "parameters");

export const envNames = (text: string): string[] => parseEnv(text).entries.map((e) => e.key);

export type Trigger =
  | { kind: "service"; typed: string }
  | { kind: "env"; typed: string }
  | { kind: "param"; typed: string }
  | { kind: "class"; typed: string; ids: boolean }
  | null;

/** What the text before the cursor on a line asks to be completed. */
export function triggerAt(before: string): Trigger {
  const env = /%env\(((?:[a-z_]+:)*)(\w*)$/.exec(before);
  if (env) return { kind: "env", typed: env[2] };
  const svc = /(?<![\w])@[?]?([\\\w.-]*)$/.exec(before);
  if (svc) return { kind: "service", typed: svc[1] };
  const param = /%([\w.-]*)$/.exec(before);
  if (param && !before.endsWith("%%")) return { kind: "param", typed: param[1] };
  const cls = /^\s*(?:-\s+)?(class|parent|decorates)\s*:\s*['"]?([\\\w]*)$/.exec(before);
  if (cls) return { kind: "class", typed: cls[2], ids: cls[1] !== "class" };
  return null;
}

/** Is a tag name written at the cursor (`name: …` or a bare item) inside `tags`? */
export function tagNameTyped(before: string, at: Position): string | null {
  if (!at.path.includes("tags")) return null;
  const m = /(?:^\s*-\s*\{?\s*|[{,]\s*)name:\s*['"]?([\w.-]*)$/.exec(before) ?? (at.currentKey === "name" ? /([\w.-]*)$/.exec(before) : null);
  if (m) return m[1];
  if (at.keyPosition && at.path[at.path.length - 1] === "-") return /^\s*-\s+([\w.-]*)$/.exec(before)?.[1] ?? null;
  return null;
}

/* ---------- Project data ---------- */

interface ProjectData {
  files: string[];
  env: string[];
}

type FsApi = Pick<typeof workspaceApi, "listDir" | "readFile">;

const CONFIG_DIRS = ["config", "config/packages", "config/services"];
const ENV_FILES = [".env", ".env.local", ".env.dev", ".env.test", ".env.prod", ".env.dist", ".env.example"];
const TTL_MS = 5000;

/** Read the other config files and `.env` files; every failure just yields less data. */
export async function readProject(fs: FsApi): Promise<ProjectData> {
  const safe = async <T>(p: Promise<T>, fallback: T): Promise<T> => p.catch(() => fallback);
  const lists = await Promise.all(CONFIG_DIRS.map((d) => safe(fs.listDir(d), [])));
  const paths = lists.flat().filter((e) => e.kind === "file" && /\.ya?ml$/i.test(e.name)).map((e) => e.path);
  const [files, env] = await Promise.all([
    Promise.all(paths.map((p) => safe(fs.readFile(p), ""))),
    Promise.all(ENV_FILES.map((p) => safe(fs.readFile(p), ""))),
  ]);
  return { files, env };
}

let cache: { at: number; data: Promise<ProjectData> } | undefined;

function projectData(): Promise<ProjectData> {
  const now = Date.now();
  if (!cache || now - cache.at > TTL_MS) {
    cache = { at: now, data: readProject(workspaceApi).catch(() => ({ files: [], env: [] })) };
  }
  return cache.data;
}

/* ---------- Completion ---------- */

const uniq = (names: string[]) => [...new Set(names)];

const options = (names: string[], type: string, detail?: string, apply?: (n: string) => string): Completion[] =>
  uniq(names).map((label) => ({ label, type, detail, apply: apply?.(label) }));

async function classOptions(typed: string): Promise<Completion[]> {
  const segment = typed.split("\\").pop() ?? "";
  const query = segment || typed;
  if (!query) return [];
  const hits = await phpApi.searchSymbols(query, "class", 50).catch(() => []);
  const prefix = typed.replace(/^\\/, "").toLowerCase();
  return hits
    .map((h) => ({ label: h.fqn.replace(/^\\/, ""), detail: h.kind, type: "class", boost: h.vendor ? -1 : 1 }))
    .filter((c) => !prefix.includes("\\") || c.label.toLowerCase().startsWith(prefix));
}

async function completeTrigger(trigger: NonNullable<Trigger>, ctx: CompletionContext): Promise<CompletionResult | null> {
  const text = ctx.state.doc.toString();
  const data = await projectData();
  const all = [text, ...data.files];
  const from = ctx.pos - trigger.typed.length;
  switch (trigger.kind) {
    case "env": {
      const line = ctx.state.doc.lineAt(ctx.pos);
      const after = line.text.slice(ctx.pos - line.from);
      const close = after.startsWith(")") ? undefined : (n: string) => `${n})%`;
      const opts = [
        ...options(ENV_PROCESSORS, "keyword", "processor", (n) => `${n}:`),
        ...options(data.env.flatMap(envNames), "variable", "env", close),
      ];
      return { from, options: opts, validFor: /^\w*$/ };
    }
    case "param": {
      const line = ctx.state.doc.lineAt(ctx.pos);
      const after = line.text.slice(ctx.pos - line.from);
      const apply = after.startsWith("%") ? undefined : (n: string) => `${n}%`;
      const names = [...KERNEL_PARAMETERS, ...all.flatMap(parameterNames)];
      return { from, options: options(names, "constant", "parameter", apply), validFor: /^[\w.-]*$/ };
    }
    case "service": {
      const ids = options(all.flatMap(serviceIds), "class", "service", undefined);
      return { from, options: [...ids, ...(await classOptions(trigger.typed))], validFor: /^[\\\w.-]*$/ };
    }
    case "class": {
      const opts = await classOptions(trigger.typed);
      if (trigger.ids) opts.push(...options(all.flatMap(serviceIds), "class", "service"));
      return opts.length ? { from, options: opts, validFor: /^[\\\w.-]*$/ } : null;
    }
  }
}

function referenceCompletion(ctx: CompletionContext): Promise<CompletionResult | null> | null {
  const line = ctx.state.doc.lineAt(ctx.pos);
  const trigger = triggerAt(line.text.slice(0, ctx.pos - line.from));
  if (!trigger) return null;
  return completeTrigger(trigger, ctx).catch(() => null);
}

const schemaSource = schemaCompletion({ root: symfonySchema });

/** Tag names only where a name is typed; the schema source covers the keys. */
function tagCompletion(ctx: CompletionContext): CompletionResult | null {
  const line = ctx.state.doc.lineAt(ctx.pos);
  const before = line.text.slice(0, ctx.pos - line.from);
  const typed = tagNameTyped(before, positionAt(ctx.state, ctx.pos));
  if (typed === null) return null;
  return { from: ctx.pos - typed.length, options: options(TAG_NAMES, "keyword", "tag"), validFor: /^[\w.-]*$/ };
}

/** Schema keys and values, except where a reference (`@`, `%`) is being typed. */
function keyCompletion(ctx: CompletionContext) {
  const line = ctx.state.doc.lineAt(ctx.pos);
  if (triggerAt(line.text.slice(0, ctx.pos - line.from))) return null;
  return schemaSource(ctx);
}

/* ---------- Highlighting overlay ---------- */

const REFERENCES =
  /(?<![\w])@=|(?<![\w])@\??[A-Za-z_\\][\w.\\-]*|%env\([^)\n]*\)%|%%|%[A-Za-z_][\w.-]*%|(?<=[\s:[,-])![A-Za-z_][\w/]*/g;

export function referenceClass(match: string): string {
  if (match.startsWith("@")) return "cm-sf-service";
  if (match.startsWith("%env(")) return "cm-sf-env";
  if (match.startsWith("%")) return "cm-sf-param";
  return "cm-sf-tag";
}

const decorator = new MatchDecorator({
  regexp: REFERENCES,
  decoration: (m) => Decoration.mark({ class: referenceClass(m[0]) }),
});

const overlay = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet;
    constructor(view: EditorView) {
      this.decorations = decorator.createDeco(view);
    }
    update(u: ViewUpdate) {
      this.decorations = decorator.updateDeco(u, this.decorations);
    }
  },
  { decorations: (v) => v.decorations },
);

/* ---------- Language ---------- */

export function symfonySupport(): Extension {
  const lang = yaml();
  return [
    lang,
    lang.language.data.of({ autocomplete: keyCompletion }),
    lang.language.data.of({ autocomplete: referenceCompletion }),
    lang.language.data.of({ autocomplete: tagCompletion }),
    schemaHover(symfonySchema),
    overlay,
  ];
}

registerLanguage({
  id: "symfony-services",
  // Outside Symfony projects (or with the plugin disabled) these files are plain YAML: the
  // file type comes from that plugin, so Rust falls back to `yaml`.
  load: () => symfonySupport(),
});
