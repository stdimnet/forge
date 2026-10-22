/**
 * Browser-mode Laravel backend over the demo project. A small line-based version of the
 * Rust analysis (`packs/phpforge/fw-laravel`): routes with groups and resources, views,
 * components, config/env keys, translations, models from migrations, completion, Go to
 * Declaration, gutter markers and the artisan commands and simulated output the Command
 * Line Tools Console mock uses. It never writes files.
 */
import { registerMock } from "@/ipc";
import {
  type GutterMark,
  type LaravelCompletionItem,
  type LaravelCompletions,
  type LaravelInfo,
  type LaravelLocation,
  type LaravelRoute,
} from "@/ipc/laravel";
import { mockFiles } from "./workspaceMock";

const files = () => mockFiles;
const paths = (prefix: string, suffix = ".php") =>
  Object.keys(files())
    .filter((p) => p.startsWith(prefix) && p.endsWith(suffix))
    .sort();

/** Location of byte/char offset `at` in `path`. */
export function locAt(path: string, at: number): LaravelLocation {
  const text = files()[path] ?? "";
  const before = text.slice(0, Math.max(0, at));
  const line = before.split("\n").length - 1;
  return { path, line, character: at - (before.lastIndexOf("\n") + 1) };
}

const fileLoc = (path: string): LaravelLocation => ({ path, line: 0, character: 0 });

function uses(src: string): Map<string, string> {
  const m = new Map<string, string>();
  for (const u of src.matchAll(/^use\s+([\w\\]+)(?:\s+as\s+(\w+))?;/gm)) {
    m.set(u[2] ?? u[1].split("\\").pop()!, u[1]);
  }
  return m;
}

function namespaceOf(src: string): string {
  return /^namespace\s+([\w\\]+);/m.exec(src)?.[1] ?? "";
}

function resolveClass(name: string, src: string): string {
  if (name.startsWith("\\")) return name.slice(1);
  const [head, ...rest] = name.split("\\");
  const imported = uses(src).get(head);
  if (imported) return [imported, ...rest].join("\\");
  const ns = namespaceOf(src);
  return ns ? `${ns}\\${name}` : name;
}

function classPath(fqn: string): string | undefined {
  const rel = fqn.replace(/^App\\/, "app/").replace(/\\/g, "/") + ".php";
  return files()[rel] !== undefined ? rel : undefined;
}

function methodLocation(fqn: string, method: string): LaravelLocation | undefined {
  const path = classPath(fqn);
  if (!path) return undefined;
  const src = files()[path];
  const i = src.search(new RegExp(`function\\s+${method}\\s*\\(`));
  if (i < 0) return locAt(path, Math.max(0, src.indexOf("class ")));
  return locAt(path, src.indexOf(method, i));
}

// ---- Routes --------------------------------------------------------------------------

interface Group {
  prefix: string;
  name: string;
  middleware: string[];
}

const strings = (s: string) => [...s.matchAll(/'([^']*)'/g)].map((m) => m[1]);
const joinUri = (a: string, b: string) => [a, b].map((x) => x.replace(/^\/+|\/+$/g, "")).filter(Boolean).join("/") || "/";
const singular = (w: string) => (w.endsWith("ies") ? `${w.slice(0, -3)}y` : w.endsWith("s") ? w.slice(0, -1) : w);

function parseRouteFile(path: string, base: Group, out: LaravelRoute[]): void {
  const src = files()[path];
  if (!src) return;
  const stack: Group[] = [base];
  let offset = 0;
  for (const line of src.split("\n")) {
    const at = offset;
    offset += line.length + 1;
    const t = line.trim();
    const g = stack[stack.length - 1];
    if (/->group\(function/.test(t) || /^Route::group\(/.test(t)) {
      const prefix = /prefix\('([^']*)'\)/.exec(t)?.[1] ?? "";
      const name = /->name\('([^']*)'\)/.exec(t)?.[1] ?? "";
      const mw = /middleware\(([^)]*)\)/.exec(t)?.[1];
      stack.push({ prefix: joinUri(g.prefix, prefix), name: g.name + name, middleware: [...g.middleware, ...(mw ? strings(mw) : [])] });
      continue;
    }
    if (t.startsWith("});")) {
      if (stack.length > 1) stack.pop();
      continue;
    }
    const m = /^Route::(get|post|put|patch|delete|view|resource|apiResource)\('([^']*)',\s*(.*)$/.exec(t);
    if (!m) continue;
    const [, verb, uri, rest] = m;
    const source = locAt(path, at + line.indexOf("Route::"));
    const mw = /->middleware\(([^)]*)\)/.exec(rest)?.[1];
    const middleware = [...g.middleware, ...(mw ? strings(mw) : [])];
    if (verb === "resource" || verb === "apiResource") {
      const cls = /^(\w+)::class/.exec(rest)?.[1];
      const controller = cls ? resolveClass(cls, src) : undefined;
      const only = /->only\(\[([^\]]*)\]\)/.exec(rest)?.[1];
      const actions: [string, string[], string][] = [
        ["index", ["GET", "HEAD"], ""],
        ["create", ["GET", "HEAD"], "/create"],
        ["store", ["POST"], ""],
        ["show", ["GET", "HEAD"], `/{${singular(uri)}}`],
        ["edit", ["GET", "HEAD"], `/{${singular(uri)}}/edit`],
        ["update", ["PUT", "PATCH"], `/{${singular(uri)}}`],
        ["destroy", ["DELETE"], `/{${singular(uri)}}`],
      ];
      for (const [action, methods, suffix] of actions) {
        if (verb === "apiResource" && (action === "create" || action === "edit")) continue;
        if (only && !strings(only).includes(action)) continue;
        out.push({
          methods,
          uri: joinUri(g.prefix, uri + suffix),
          name: `${g.name}${uri}.${action}`,
          action: controller ? `${controller}@${action}` : action,
          controller,
          controllerMethod: action,
          middleware,
          source,
          actionLocation: controller ? methodLocation(controller, action) : undefined,
        });
      }
      continue;
    }
    const name = /->name\('([^']*)'\)/.exec(rest)?.[1];
    let action = "Closure";
    let controller: string | undefined;
    let controllerMethod: string | undefined;
    if (verb === "view") action = `view: ${strings(rest)[0] ?? ""}`;
    const arr = /^\[(\w+)::class,\s*'(\w+)'\]/.exec(rest);
    if (arr) {
      controller = resolveClass(arr[1], src);
      controllerMethod = arr[2];
      action = `${controller}@${controllerMethod}`;
    }
    out.push({
      methods: verb === "get" || verb === "view" ? ["GET", "HEAD"] : [verb.toUpperCase()],
      uri: joinUri(g.prefix, uri),
      name: name ? g.name + name : undefined,
      action,
      controller,
      controllerMethod,
      middleware,
      source,
      actionLocation: controller && controllerMethod ? methodLocation(controller, controllerMethod) : undefined,
    });
  }
}

export function mockRoutes(): LaravelRoute[] {
  const out: LaravelRoute[] = [];
  parseRouteFile("routes/web.php", { prefix: "", name: "", middleware: ["web"] }, out);
  parseRouteFile("routes/api.php", { prefix: "api", name: "", middleware: ["api"] }, out);
  // Framework routes that `artisan route:list` also shows.
  out.push({ methods: ["GET", "HEAD"], uri: "up", action: "Closure", middleware: [] });
  out.push({
    methods: ["GET", "HEAD"],
    uri: "storage/{path}",
    name: "storage.local",
    action: "Illuminate\\Filesystem\\ServeFile",
    middleware: [],
  });
  return out;
}

// ---- Views, components, config, env, translations -------------------------------------

export const mockViews = () =>
  paths("resources/views/", ".blade.php").map((path) => ({
    name: path.slice("resources/views/".length, -".blade.php".length).replace(/\//g, "."),
    path,
  }));

const viewPath = (name: string) => {
  const p = `resources/views/${name.replace(/\./g, "/")}.blade.php`;
  return files()[p] !== undefined ? p : undefined;
};

const kebab = (s: string) => s.replace(/([a-z0-9])([A-Z])/g, "$1-$2").toLowerCase();

export function mockComponents() {
  const out = new Map<string, { tag: string; view?: string; class?: string; classPath?: string; props: string[] }>();
  for (const view of paths("resources/views/components/", ".blade.php")) {
    const tag = `x-${view.slice("resources/views/components/".length, -".blade.php".length).replace(/\//g, ".")}`;
    const props = /@props\(\[([^\]]*)\]\)/.exec(files()[view])?.[1];
    out.set(tag, { tag, view, props: props ? [...props.matchAll(/'(\w+)'\s*(?:=>|,|$)/g)].map((m) => m[1]) : [] });
  }
  for (const cls of paths("app/View/Components/")) {
    const name = cls.slice("app/View/Components/".length, -4);
    const tag = `x-${name.split("/").map(kebab).join(".")}`;
    const src = files()[cls];
    const props = [...(/__construct\(([^)]*)\)/.exec(src)?.[1] ?? "").matchAll(/\$(\w+)/g)].map((m) => kebab(m[1]));
    out.set(tag, { ...out.get(tag), tag, class: `App\\View\\Components\\${name.replace(/\//g, "\\")}`, classPath: cls, props });
  }
  return [...out.values()].sort((a, b) => a.tag.localeCompare(b.tag));
}

/** `'key' => value` entries of a returned PHP array, flattened by indentation. */
function phpArrayKeys(path: string, prefix: string): { key: string; value: string; location: LaravelLocation }[] {
  const src = files()[path] ?? "";
  const out: { key: string; value: string; location: LaravelLocation }[] = [];
  const stack: string[] = [];
  let offset = 0;
  for (const line of src.split("\n")) {
    const at = offset;
    offset += line.length + 1;
    const m = /^(\s*)'([^']+)'\s*=>\s*(.*?),?$/.exec(line);
    const depth = Math.max(0, Math.floor((line.length - line.trimStart().length) / 4) - 1);
    if (/^\s*\],?$/.test(line)) continue;
    if (!m) continue;
    stack.length = depth;
    const key = [prefix, ...stack, m[2]].join(".");
    const value = m[3] === "[" ? "array" : m[3];
    out.push({ key, value, location: locAt(path, at + m[1].length) });
    if (m[3] === "[") stack.push(m[2]);
  }
  return out;
}

export const mockConfig = () =>
  paths("config/").flatMap((p) => {
    const prefix = p.slice("config/".length, -4);
    return [{ key: prefix, value: "array", location: fileLoc(p) }, ...phpArrayKeys(p, prefix)];
  });

export function mockEnv() {
  const out: { key: string; value: string; location: LaravelLocation }[] = [];
  for (const path of [".env", ".env.example"]) {
    const src = files()[path] ?? "";
    let offset = 0;
    for (const line of src.split("\n")) {
      const at = offset;
      offset += line.length + 1;
      const m = /^([A-Z0-9_]+)=(.*)$/.exec(line.trim());
      if (m && !out.some((e) => e.key === m[1])) out.push({ key: m[1], value: m[2].replace(/^"|"$/g, ""), location: locAt(path, at) });
    }
  }
  return out;
}

export const mockTranslations = () =>
  paths("lang/en/").flatMap((p) =>
    phpArrayKeys(p, p.slice("lang/en/".length, -4))
      .filter((e) => e.value !== "array")
      .map((e) => ({ ...e, value: e.value.replace(/^'|'$/g, "") })),
  );

// ---- Models --------------------------------------------------------------------------

interface MockModel {
  class: string;
  name: string;
  table: string;
  attributes: { name: string; type: string; location?: LaravelLocation }[];
  relations: { name: string; kind: string; related?: string; location: LaravelLocation }[];
  scopes: { name: string; location: LaravelLocation }[];
}

const COLUMN_TYPES: Record<string, string> = {
  id: "int", foreignId: "int", integer: "int", unsignedInteger: "int", bigInteger: "int", boolean: "bool",
  decimal: "float", float: "float", double: "float", timestamp: "Carbon", dateTime: "Carbon", date: "Carbon", json: "array",
};

function tableColumns(table: string): { name: string; type: string; location: LaravelLocation }[] {
  const cols: { name: string; type: string; location: LaravelLocation }[] = [];
  for (const path of paths("database/migrations/")) {
    const src = files()[path];
    const start = src.indexOf(`Schema::create('${table}'`);
    if (start < 0) continue;
    const body = src.slice(start, src.indexOf("});", start));
    for (const m of body.matchAll(/\$table->(\w+)\(([^)]*)\)/g)) {
      const loc = locAt(path, start + (m.index ?? 0));
      const [type, args] = [m[1], m[2]];
      const name = strings(args)[0];
      if (type === "id") cols.push({ name: "id", type: "int", location: loc });
      else if (type === "timestamps") cols.push({ name: "created_at", type: "Carbon", location: loc }, { name: "updated_at", type: "Carbon", location: loc });
      else if (type === "rememberToken") cols.push({ name: "remember_token", type: "string", location: loc });
      else if (type === "foreignIdFor") cols.push({ name: `${kebab(/(\w+)::class/.exec(args)?.[1] ?? "x").replace(/-/g, "_")}_id`, type: "int", location: loc });
      else if (name) cols.push({ name, type: COLUMN_TYPES[type] ?? "string", location: loc });
    }
  }
  return cols;
}

export function mockModels(): MockModel[] {
  return paths("app/Models/")
    .filter((p) => /extends (Model|Authenticatable)/.test(files()[p]))
    .map((path) => {
      const src = files()[path];
      const name = path.slice("app/Models/".length, -4);
      const table = kebab(name).replace(/-/g, "_") + "s";
      const attributes: MockModel["attributes"] = tableColumns(table);
      const add = (n: string, type: string, at?: number) => {
        if (!attributes.some((a) => a.name === n)) attributes.push({ name: n, type, location: at != null ? locAt(path, at) : undefined });
      };
      for (const f of strings(/\$fillable\s*=\s*\[([^\]]*)\]/.exec(src)?.[1] ?? "")) add(f, "mixed", src.indexOf("$fillable"));
      for (const c of (/\$casts\s*=\s*\[([^\]]*)\]/.exec(src)?.[1] ?? "").matchAll(/'(\w+)'\s*=>\s*([^,\]]+)/g)) {
        add(c[1], "mixed");
        const a = attributes.find((x) => x.name === c[1])!;
        a.type = c[2].includes("::class") ? c[2].replace("::class", "").trim() : c[2].includes("datetime") ? "Carbon" : c[2].includes("decimal") ? "float" : a.type;
      }
      const relations: MockModel["relations"] = [];
      for (const r of src.matchAll(/function (\w+)\(\)[^{]*\{\s*return \$this->(hasMany|hasOne|belongsTo|belongsToMany)\((\w+)::class/g)) {
        relations.push({ name: r[1], kind: r[2], related: `App\\Models\\${r[3]}`, location: locAt(path, (r.index ?? 0) + 9) });
      }
      const scopes = [...src.matchAll(/function scope(\w+)\(/g)].map((s) => ({
        name: s[1][0].toLowerCase() + s[1].slice(1),
        location: locAt(path, (s.index ?? 0) + 9),
      }));
      return { class: `App\\Models\\${name}`, name, table, attributes, relations, scopes };
    });
}

// ---- Editor services -----------------------------------------------------------------

const ROUTE_FUNCS = ["route", "to_route", "routeIs"];
const VIEW_FUNCS = ["view", "@include", "@extends", "@includeIf", "@each", "@component"];
const COLUMN_FUNCS = ["where", "orWhere", "whereIn", "orderBy", "orderByDesc", "latest", "oldest", "pluck", "value", "sum", "firstWhere", "select", "groupBy"];
const RELATION_FUNCS = ["with", "load", "has", "whereHas", "withCount", "doesntHave"];

type Ctx =
  | { kind: "string"; func: string; receiver: string; from: number; to: number; value: string }
  | { kind: "property"; variable: string; from: number; to: number; word: string }
  | { kind: "static"; cls: string; from: number; to: number; word: string }
  | { kind: "component"; from: number; to: number; value: string }
  | { kind: "directive"; from: number; to: number; value: string };

export function contextAt(text: string, offset: number): Ctx | null {
  const lineStart = text.lastIndexOf("\n", offset - 1) + 1;
  const before = text.slice(lineStart, offset);
  const lineEnd = text.indexOf("\n", offset) < 0 ? text.length : text.indexOf("\n", offset);
  const after = text.slice(offset, lineEnd);
  const str = /([@\w]+)\s*\(\s*(?:[^'"()]*,\s*)?(['"])([^'"]*)$/.exec(before);
  if (str) {
    const close = after.indexOf(str[2]);
    const to = close < 0 ? offset : offset + close;
    const head = before.slice(0, str.index);
    const recv = /((?:\$\w+|\w+::\w+\(\)|\w+)(?:\??->\w+\([^()]*\))*)(?:->|::)$/.exec(head)?.[1] ?? "";
    return { kind: "string", func: str[1], receiver: recv, from: offset - str[3].length, to, value: str[3] + (close < 0 ? "" : after.slice(0, close)) };
  }
  const word = /[\w]*$/.exec(before)![0];
  const rest = /^[\w]*/.exec(after)![0];
  const prop = /(\$\w+)->(\w*)$/.exec(before);
  if (prop) return { kind: "property", variable: prop[1], from: offset - word.length, to: offset + rest.length, word: word + rest };
  const stat = /([A-Z]\w*)::(\w*)$/.exec(before);
  if (stat) return { kind: "static", cls: stat[1], from: offset - word.length, to: offset + rest.length, word: word + rest };
  const comp = /<\/?((?:x-|livewire:)[\w.:-]*)$/.exec(before) ?? (/<(x?)$/.exec(before) as RegExpExecArray | null);
  if (comp) {
    const tail = /^[\w.:-]*/.exec(after)![0];
    return { kind: "component", from: offset - comp[1].length, to: offset + tail.length, value: comp[1] + tail };
  }
  const dir = /(^|[^\w@])(@\w*)$/.exec(before);
  if (dir) return { kind: "directive", from: offset - dir[2].length, to: offset + rest.length, value: dir[2] + rest };
  return null;
}

const item = (label: string, kind: LaravelCompletionItem["kind"], detail?: string, documentation?: string): LaravelCompletionItem => ({
  label,
  kind,
  detail,
  documentation,
});

function modelFor(name: string, text: string): MockModel | undefined {
  const fqn = resolveClass(name, text);
  return mockModels().find((m) => m.class === fqn || m.name === name);
}

function variableModel(variable: string, text: string): MockModel | undefined {
  const typed = new RegExp(`(\\w+)\\s+\\${variable}\\b`).exec(text)?.[1];
  if (typed && /^[A-Z]/.test(typed)) {
    const m = modelFor(typed, text);
    if (m) return m;
  }
  const bare = variable.slice(1);
  return mockModels().find((m) => m.name.toLowerCase() === bare.toLowerCase() || `${m.name.toLowerCase()}s` === bare.toLowerCase());
}

function receiverModel(receiver: string, text: string): MockModel | undefined {
  const cls = /^(\w+)(?:::|$)/.exec(receiver)?.[1];
  if (cls && /^[A-Z]/.test(cls)) return modelFor(cls, text);
  const v = /^\$\w+/.exec(receiver)?.[0];
  return v ? variableModel(v, text) : undefined;
}

export function mockComplete(path: string, text: string, offset: number): LaravelCompletions | null {
  const ctx = contextAt(text, offset);
  if (!ctx) return null;
  let items: LaravelCompletionItem[] = [];
  if (ctx.kind === "string") {
    const f = ctx.func;
    const recvClass = /^(\w+)/.exec(ctx.receiver)?.[1];
    if (ROUTE_FUNCS.includes(f)) {
      items = mockRoutes()
        .filter((r) => r.name)
        .map((r) => item(r.name!, "route", `${r.methods.join("|")} /${r.uri.replace(/^\//, "")}`, r.action));
    } else if (VIEW_FUNCS.includes(f) || (recvClass === "View" && f === "make")) {
      items = mockViews().map((v) => item(v.name, "view", v.path));
    } else if (f === "config" || (recvClass === "Config" && f === "get")) {
      items = mockConfig().map((c) => item(c.key, "config", c.value));
    } else if (f === "env") {
      items = mockEnv().map((e) => item(e.key, "env", e.value));
    } else if (f === "__" || f === "trans" || f === "@lang") {
      items = mockTranslations().map((t) => item(t.key, "translation", t.value));
    } else if (COLUMN_FUNCS.includes(f) && ctx.receiver) {
      const m = receiverModel(ctx.receiver, text);
      items = m?.attributes.map((a) => item(a.name, "column", a.type, `${m.table}.${a.name}`)) ?? [];
    } else if (RELATION_FUNCS.includes(f) && ctx.receiver) {
      const m = receiverModel(ctx.receiver, text);
      items = m?.relations.map((r) => item(r.name, "relation", `${r.kind} → ${r.related?.split("\\").pop()}`)) ?? [];
    } else return null;
  } else if (ctx.kind === "property") {
    const m = variableModel(ctx.variable, text);
    if (!m) return null;
    items = [
      ...m.attributes.map((a) => item(a.name, "property", a.type, `${m.table}.${a.name}`)),
      ...m.relations.map((r) => item(r.name, "property", r.kind.includes("Many") ? `Collection<${r.related?.split("\\").pop()}>` : r.related?.split("\\").pop(), `${r.kind} relation`)),
    ];
  } else if (ctx.kind === "static") {
    const m = modelFor(ctx.cls, text);
    if (!m) return null;
    items = m.scopes.map((s) => ({ ...item(s.name, "method", "scope"), insertText: `${s.name}()` }));
  } else if (ctx.kind === "component") {
    if (!path.endsWith(".blade.php")) return null;
    items = mockComponents().map((c) => item(c.tag, "component", c.class ?? c.view, c.props.length ? `Attributes: ${c.props.join(", ")}` : undefined));
  } else if (ctx.kind === "directive") {
    if (!path.endsWith(".blade.php")) return null;
    items = [item("@money", "directive", "custom directive")];
  }
  return { from: ctx.from, to: ctx.to, items };
}

export function mockGoto(_path: string, text: string, offset: number): LaravelLocation[] {
  const ctx = contextAt(text, offset);
  if (!ctx) return [];
  if (ctx.kind === "string") {
    const f = ctx.func;
    if (ROUTE_FUNCS.includes(f)) {
      return mockRoutes()
        .filter((r) => r.name === ctx.value)
        .flatMap((r) => [r.source, r.actionLocation].filter((l): l is LaravelLocation => !!l));
    }
    if (VIEW_FUNCS.includes(f)) {
      const p = viewPath(ctx.value);
      return p ? [fileLoc(p)] : [];
    }
    if (f === "config") return mockConfig().filter((c) => c.key === ctx.value).map((c) => c.location);
    if (f === "env") return mockEnv().filter((e) => e.key === ctx.value).map((e) => e.location);
    if (f === "__" || f === "trans") return mockTranslations().filter((t) => t.key === ctx.value).map((t) => t.location);
    if (COLUMN_FUNCS.includes(f)) {
      const a = receiverModel(ctx.receiver, text)?.attributes.find((x) => x.name === ctx.value);
      return a?.location ? [a.location] : [];
    }
    return [];
  }
  if (ctx.kind === "property") {
    const m = variableModel(ctx.variable, text);
    const r = m?.relations.find((x) => x.name === ctx.word);
    if (r) return [r.location];
    const a = m?.attributes.find((x) => x.name === ctx.word);
    return a?.location ? [a.location] : [];
  }
  if (ctx.kind === "static") {
    const s = modelFor(ctx.cls, text)?.scopes.find((x) => x.name === ctx.word);
    return s ? [s.location] : [];
  }
  if (ctx.kind === "component") {
    const c = mockComponents().find((x) => x.tag === ctx.value);
    return c ? [c.classPath, c.view].filter((p): p is string => !!p).map(fileLoc) : [];
  }
  if (ctx.kind === "directive" && ctx.value === "@money") {
    const p = "app/Providers/AppServiceProvider.php";
    return [locAt(p, files()[p].indexOf("'money'") + 1)];
  }
  return [];
}

export function mockGutter(path: string, text: string): GutterMark[] {
  const marks: GutterMark[] = [];
  const lineOf = (at: number) => text.slice(0, at).split("\n").length - 1;
  const routes = mockRoutes();
  if (path.startsWith("app/") && /class\s+\w+/.test(text)) {
    const fqn = `${namespaceOf(text)}\\${/class\s+(\w+)/.exec(text)![1]}`;
    for (const m of text.matchAll(/function\s+(\w+)\s*\(/g)) {
      const rs = routes.filter((r) => r.controller === fqn && r.controllerMethod === m[1]);
      if (!rs.length) continue;
      marks.push({
        line: lineOf(m.index ?? 0),
        kind: "route",
        tooltip: rs.map((r) => `${r.methods.join("|")} /${r.uri.replace(/^\//, "")}${r.name ? ` (${r.name})` : ""}`).join("\n"),
        targets: rs.flatMap((r) => (r.source ? [r.source] : [])),
      });
    }
  }
  if (path.startsWith("routes/")) {
    for (const r of routes) {
      if (r.source?.path !== path || !r.actionLocation || marks.some((m) => m.line === r.source!.line)) continue;
      marks.push({
        line: r.source.line,
        kind: "controller",
        tooltip: `Navigate to ${r.controller?.split("\\").pop()}::${r.controllerMethod}()`,
        targets: [r.actionLocation],
      });
    }
  }
  for (const m of text.matchAll(/(?<![\w:>])view\(\s*'([^']+)'|@(?:include|extends)\(\s*'([^']+)'/g)) {
    const name = m[1] ?? m[2];
    const p = viewPath(name);
    const line = lineOf(m.index ?? 0);
    if (p && !marks.some((x) => x.line === line)) {
      marks.push({ line, kind: "view", tooltip: `Navigate to view ${name}`, targets: [fileLoc(p)] });
    }
  }
  return marks.sort((a, b) => a.line - b.line);
}

// ---- Artisan -------------------------------------------------------------------------

/** A demo artisan command (`cliToolsMock` turns the list into `artisan list --format=json`). */
export interface ArtisanCommand {
  name: string;
  description: string;
  usage: string;
  arguments: string[];
  options: string[];
}

const cmd = (name: string, description: string, args: string[] = [], options: string[] = []): ArtisanCommand => ({
  name,
  description,
  usage: `${name} [options]${args.length ? ` [--] ${args.map((a) => `<${a}>`).join(" ")}` : ""}`,
  arguments: args,
  options,
});

export const mockArtisanCommands: ArtisanCommand[] = [
  cmd("about", "Display basic information about your application", [], ["only", "json"]),
  cmd("cache:clear", "Flush the application cache", ["store"], ["tags"]),
  cmd("config:cache", "Create a cache file for faster configuration loading"),
  cmd("config:clear", "Remove the configuration cache file"),
  cmd("config:show", "Display all of the values for a given configuration file or key", ["config"]),
  cmd("db:seed", "Seed the database with records", ["class"], ["force"]),
  cmd("db:show", "Display information about the given database", [], ["counts", "views"]),
  cmd("down", "Put the application into maintenance / demo mode", [], ["secret", "render"]),
  cmd("inspire", "Display an inspiring quote"),
  cmd("key:generate", "Set the application key", [], ["show", "force"]),
  cmd("make:component", "Create a new view component class", ["name"], ["view", "inline"]),
  cmd("make:controller", "Create a new controller class", ["name"], ["api", "invokable", "model", "resource"]),
  cmd("make:migration", "Create a new migration file", ["name"], ["create", "table"]),
  cmd("make:model", "Create a new Eloquent model class", ["name"], ["all", "controller", "factory", "migration", "seed"]),
  cmd("make:request", "Create a new form request class", ["name"]),
  cmd("make:test", "Create a new test class", ["name"], ["pest", "unit"]),
  cmd("migrate", "Run the database migrations", [], ["force", "pretend", "seed", "step"]),
  cmd("migrate:fresh", "Drop all tables and re-run all migrations", [], ["seed"]),
  cmd("migrate:rollback", "Rollback the last database migration", [], ["step"]),
  cmd("migrate:status", "Show the status of each migration"),
  cmd("model:show", "Show information about an Eloquent model", ["model"]),
  cmd("optimize", "Cache framework bootstrap, configuration, and metadata to increase performance"),
  cmd("optimize:clear", "Remove the cached bootstrap files"),
  cmd("queue:work", "Start processing jobs on the queue as a daemon", ["connection"], ["queue", "once", "tries"]),
  cmd("route:cache", "Create a route cache file for faster route registration"),
  cmd("route:clear", "Remove the route cache file"),
  cmd("route:list", "List all registered routes", [], ["json", "method", "name", "path", "except-vendor"]),
  cmd("schedule:list", "List all scheduled tasks"),
  cmd("serve", "Serve the application on the PHP development server", [], ["host", "port"]),
  cmd("storage:link", "Create the symbolic links configured for the application"),
  cmd("test", "Run the application tests", [], ["coverage", "parallel", "filter"]),
  cmd("tinker", "Interact with your application"),
  cmd("up", "Bring the application out of maintenance mode"),
  cmd("view:cache", "Compile all of the application's Blade templates"),
  cmd("view:clear", "Clear all compiled view files"),
];

/** Output of `php artisan <line>` in the demo project: `[text, stderr]` lines. */
export function simulatedArtisanOutput(line: string): { lines: [string, boolean][]; code: number } {
  const [name, ...args] = line.replace(/^(php\s+)?artisan\s+/, "").trim().split(/\s+/);
  const ok = (text: string): [string, boolean] => [text, false];
  switch (name) {
    case "route:list": {
      const rows = mockRoutes().map((r) => {
        const left = `  ${r.methods.join("|").padEnd(10)} ${r.uri}`;
        const right = `${r.name ? `${r.name} › ` : ""}${r.action.replace("App\\Http\\Controllers\\", "")}`;
        return ok(`${left} ${".".repeat(Math.max(2, 92 - left.length - right.length))} ${right}`);
      });
      return { lines: [ok(""), ...rows, ok(""), ok(`${" ".repeat(78)}Showing [${rows.length}] routes`), ok("")], code: 0 };
    }
    case "migrate:status":
      return {
        lines: [
          ok(""),
          ok("  Migration name .............................................. Batch / Status"),
          ...paths("database/migrations/").map((p) => ok(`  ${p.split("/").pop()!.replace(".php", "").padEnd(60, " ")}[1] Ran`)),
          ok(""),
        ],
        code: 0,
      };
    case "about":
      return {
        lines: [
          ok(""),
          ok("  Environment ......................................................"),
          ok("  Application Name ......................................... Acme Shop"),
          ok("  Laravel Version ............................................. 12.4.1"),
          ok("  PHP Version ................................................. 8.3.14"),
          ok("  Composer Version ............................................. 2.8.4"),
          ok("  Environment ................................................... local"),
          ok("  Debug Mode ................................................... ENABLED"),
          ok(""),
        ],
        code: 0,
      };
    case "inspire":
      return { lines: [ok(""), ok("  “ Simplicity is the ultimate sophistication. ”"), ok("  — Leonardo da Vinci"), ok("")], code: 0 };
    default:
      if (name?.startsWith("make:") && args[0]) {
        const kind = name.slice(5);
        const target =
          kind === "model"
            ? `app/Models/${args[0]}.php`
            : kind === "controller"
              ? `app/Http/Controllers/${args[0]}.php`
              : kind === "migration"
                ? `database/migrations/2026_10_23_091402_${args[0]}.php`
                : args[0];
        const label = kind.charAt(0).toUpperCase() + kind.slice(1);
        return { lines: [ok(""), ok(`   \x1b[44;37m INFO \x1b[0m ${label} [${target}] created successfully.`), ok("")], code: 0 };
      }
      if (!mockArtisanCommands.some((c) => c.name === name)) {
        return { lines: [["", true], [`   ERROR  Command "${name ?? ""}" is not defined.`, true], ["", true]], code: 1 };
      }
      return { lines: [ok(""), ok(`   INFO  ${name} finished (simulated).`), ok("")], code: 0 };
  }
}

registerMock("laravel_detect", (): LaravelInfo => ({
  detected: true,
  version: "12.4.1",
  routes: mockRoutes().length,
  views: mockViews().length,
  components: mockComponents().length,
  models: mockModels().length,
  configKeys: mockConfig().length,
  translations: mockTranslations().length,
  directives: ["money"],
}));
registerMock("laravel_routes", () => ({ routes: mockRoutes(), source: "static" }));
registerMock("laravel_complete", ({ path, offset, content }) =>
  mockComplete(String(path), String(content ?? files()[String(path)] ?? ""), Number(offset)),
);
registerMock("laravel_goto", ({ path, offset, content }) =>
  mockGoto(String(path), String(content ?? files()[String(path)] ?? ""), Number(offset)),
);
registerMock("laravel_gutter", ({ path, content }) => mockGutter(String(path), String(content ?? files()[String(path)] ?? "")));
