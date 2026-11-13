/**
 * Browser-mode Laravel tool window: the overview of the demo shop, `storage/logs/*.log`
 * parsed like `phpforge_fw_laravel::logs` (simplified) and `ide.json` code generations,
 * all over the in-memory demo files (generated files are added there, never to disk).
 */
import { registerMock } from "@/ipc";
import type {
  CodeGeneration,
  GeneratedFile,
  LaravelAppCommand,
  LaravelLogEntry,
  LaravelLogFile,
  LaravelLogPage,
  LaravelOverview,
  LogLevel,
  LogStackFrame,
} from "@/ipc/laravel";
import { locAt, mockComponents, mockConfig, mockEnv, mockModels, mockRoutes, mockTranslations, mockViews } from "./laravelMock";
import { mockFiles } from "./workspaceMock";

const LEVELS: LogLevel[] = ["debug", "info", "notice", "warning", "error", "critical", "alert", "emergency"];

/** `/var/www/html/app/X.php` -> `app/X.php` when that file is in the project. */
function mapPath(file: string): string | undefined {
  const parts = file.replace(/\\/g, "/").split("/");
  for (let i = 1; i < parts.length; i++) {
    const rest = parts.slice(i).join("/");
    if (rest in mockFiles || /^(vendor|public)\//.test(rest)) return rest;
  }
  return undefined;
}

const unescape = (s: string) => s.replace(/\\\\/g, "\\").replace(/\\"/g, '"');

function frame(line: string): LogStackFrame | null {
  const m = /^#\d+ (.*)$/.exec(line.trim().replace(/^"/, ""));
  if (!m) return null;
  if (m[1] === "{main}") return { call: "{main}" };
  const internal = /^\[internal function\]: (.*)$/.exec(m[1]);
  if (internal) return { call: unescape(internal[1]) };
  const f = /^(.*)\((\d+)\): (.*)$/.exec(m[1]);
  if (!f) return null;
  const path = mapPath(f[1]);
  return { file: f[1], path, line: Number(f[2]), call: unescape(f[3]), vendor: (path ?? f[1]).includes("vendor/") };
}

export function parseLog(text: string): LaravelLogEntry[] {
  const out: LaravelLogEntry[] = [];
  let body: string[] = [];
  const finish = () => {
    const e = out[out.length - 1];
    if (!e) return;
    const full = [e.message, ...body].join("\n");
    const x = /\[object\] \(([^(\s]+)\(code: (\S+)\): (.*) at (\S+):(\d+)\)/.exec(full);
    if (x) {
      const path = mapPath(x[4]);
      e.exception = {
        class: unescape(x[1]),
        message: unescape(x[3]),
        code: x[2] === "0" ? undefined : x[2],
        frame: { file: x[4], path, line: Number(x[5]), call: "", vendor: (path ?? x[4]).includes("vendor/") },
      };
      e.message = e.message.replace(/ \{"exception".*$/, "");
      e.context = undefined;
    }
    e.frames = body.map(frame).filter((f): f is LogStackFrame => !!f);
    body = [];
  };
  text.split("\n").forEach((line, i) => {
    const h = /^\[(\d{4}-\d{2}-\d{2}[ T][\d:]+)[^\]]*\] ([\w-]+)\.(\w+): (.*)$/.exec(line);
    const level = h?.[3].toLowerCase() as LogLevel | undefined;
    if (h && level && LEVELS.includes(level)) {
      finish();
      const ctx = /^(.*?) (\{".*|\[\{.*|\{\} \[\]|\[\] \[\])$/.exec(h[4]);
      const context = ctx && !ctx[2].startsWith('{"exception"') && ctx[2] !== "[] []" ? ctx[2] : undefined;
      out.push({ time: h[1].replace("T", " "), channel: h[2], level, message: ctx && !ctx[2].startsWith('{"exception"') ? ctx[1] : h[4], context, line: i });
    } else if (out.length) body.push(line);
  });
  finish();
  return out;
}

function logFiles(): LaravelLogFile[] {
  return Object.keys(mockFiles)
    .filter((p) => /^storage\/logs\/[^/]+\.log$/.test(p))
    .sort()
    .map((path) => ({ path, size: mockFiles[path].length, modified: 1793000000 }));
}

export function logPage(path?: string): LaravelLogPage {
  const files = logFiles();
  const file = path || files[0]?.path || "";
  const text = mockFiles[file] ?? "";
  return { path: file, files, entries: file ? parseLog(text) : [], size: text.length };
}

function codeGenerations(): { generations: CodeGeneration[]; completions: number; files: string[] } {
  const text = mockFiles["ide.json"];
  if (!text) return { generations: [], completions: 0, files: [] };
  try {
    const json = JSON.parse(text) as { completions?: unknown[]; codeGenerations?: { id?: string; name: string; classSuffix?: string; files?: { appNamespace?: string; name: string; template?: { type?: string; path?: string } }[] }[] };
    const generations = (json.codeGenerations ?? []).map((g) => ({
      id: g.id ?? g.name,
      name: g.name,
      classSuffix: g.classSuffix,
      source: "ide.json",
      files: (g.files ?? []).map((f) => ({ appNamespace: f.appNamespace ?? "", name: f.name, templateType: f.template?.type ?? "stub", templatePath: (f.template?.path ?? "").replace(/^\//, "") })),
    }));
    return { generations, completions: json.completions?.length ?? 0, files: ["ide.json"] };
  } catch {
    return { generations: [], completions: 0, files: ["ide.json"] };
  }
}

function commands(): LaravelAppCommand[] {
  const out: LaravelAppCommand[] = [];
  for (const path of Object.keys(mockFiles).filter((p) => p.startsWith("app/Console/Commands/") && p.endsWith(".php"))) {
    const src = mockFiles[path];
    const sig = /\$signature\s*=\s*'([^']+)'/.exec(src);
    if (!sig) continue;
    out.push({ name: sig[1].split(/\s/)[0], signature: sig[1], description: /\$description\s*=\s*'([^']+)'/.exec(src)?.[1], location: locAt(path, src.indexOf(sig[1])) });
  }
  const console = mockFiles["routes/console.php"] ?? "";
  for (const m of console.matchAll(/Artisan::command\('([^']+)'/g)) {
    out.push({ name: m[1].split(/\s/)[0], signature: m[1], description: /->purpose\('([^']+)'\)/.exec(console.slice(m.index))?.[1], location: locAt("routes/console.php", (m.index ?? 0) + 17) });
  }
  return out.sort((a, b) => a.name.localeCompare(b.name));
}

export function overview(): LaravelOverview {
  const env = (k: string) => mockEnv().find((e) => e.key === k)?.value.replace(/^"|"$/g, "");
  const composer = (() => {
    try {
      return JSON.parse(mockFiles["composer.json"] ?? "{}") as { require?: Record<string, string> };
    } catch {
      return {};
    }
  })();
  const ide = codeGenerations();
  return {
    version: "12.4.1",
    php: composer.require?.php,
    appName: env("APP_NAME"),
    environment: env("APP_ENV"),
    debug: env("APP_DEBUG") === undefined ? undefined : env("APP_DEBUG") === "true",
    url: env("APP_URL"),
    routes: mockRoutes().length,
    views: mockViews().length,
    components: mockComponents().length,
    translations: mockTranslations().length,
    configKeys: mockConfig().length,
    models: mockModels().map((m) => ({
      name: m.name,
      class: m.class,
      table: m.table,
      attributes: m.attributes.length,
      relations: m.relations.length,
      location: { path: `app/Models/${m.name}.php`, line: 0, character: 0 },
    })),
    commands: commands(),
    logFiles: logFiles(),
    ideJson: { files: ide.files, completions: ide.completions, codeGenerations: ide.generations },
  };
}

const words = (s: string) => s.replace(/([a-z0-9])([A-Z])/g, "$1 $2").split(/[^A-Za-z0-9]+/).filter(Boolean);
const studly = (s: string) => words(s).map((w) => w[0].toUpperCase() + w.slice(1).toLowerCase()).join("");

export function generate(id: string, input: string, write: boolean): GeneratedFile[] {
  const g = codeGenerations().generations.find((x) => x.id === id);
  if (!g) return [];
  let base = input.trim().replace(/\//g, "\\").replace(/^\\+|\\+$/g, "");
  if (g.classSuffix && !base.endsWith(g.classSuffix)) base += g.classSuffix;
  const parts = base.split("\\");
  const cls = studly(parts.pop()!);
  return g.files.map((f) => {
    const ns = ["App", f.appNamespace, ...parts].filter(Boolean).join("\\");
    const path = `app/${[f.appNamespace, ...parts].filter(Boolean).join("/")}/${cls}.php`.replace(/\/+/g, "/");
    const content = (mockFiles[f.templatePath] ?? "").replace(/\$\{NAMESPACE\}/g, ns).replace(/\$\{CLASS\}/g, cls);
    const exists = path in mockFiles;
    if (write && !exists) mockFiles[path] = content;
    return { path, content, exists };
  });
}

registerMock("laravel_overview", () => overview());
registerMock("laravel_logs", ({ path }) => logPage(path ? String(path) : undefined));
registerMock("laravel_generate", ({ id, input, write }) => generate(String(id), String(input), !!write));
