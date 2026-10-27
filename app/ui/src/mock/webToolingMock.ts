/**
 * Browser-mode web tooling: File Watchers, import aliases, OpenAPI, GraphQL, Nx, Nuxt,
 * Deno, Live Edit and cURL import/export, computed by the same Rust crates as the desktop
 * commands (WebAssembly) over the in-memory project. Watcher runs print canned compiler
 * output and write the compiled file into the in-memory project; nothing touches disk.
 * `?project=webtools` opens the demo (demoWebTooling.ts) with an SCSS watcher configured.
 */
import { emitMock, registerMock } from "@/ipc";
import type { RunEvent, RunRequest } from "@/ipc/run";
import { callWasm, callWasmSync, forgeWasm } from "@/wasm";
import { mockFiles, mockProjectName } from "./workspaceMock";
import { mockSettingValue, setSetting } from "./settingsMock";
import { mockSafeMode } from "./trustMock";
import { registerMockRunHandler, projectRoot } from "./runMock";
import { webDemoId } from "./demoWebProjects";
import { webToolingWatchers } from "./demoWebTooling";

const wasm = () => forgeWasm();
const root = () => projectRoot();

const configured = mockSettingValue("tools.fileWatchers");
if (webDemoId() === "webtools" && (!Array.isArray(configured) || configured.length === 0)) {
  setSetting("tools.fileWatchers", webToolingWatchers as never, "project");
}

/** Script and manifest files the alias resolution reads (like `fs::wants_content`). */
const CONTENT = /\.(m?[jt]sx?|cjs|vue|html?)$|(^|\/)(package|angular|jsconfig|tsconfig[\w.]*)\.json$/;
const project = () => ({
  files: Object.entries(mockFiles)
    .filter(([p]) => CONTENT.test(p) && !p.includes("node_modules/"))
    .map(([path, content]) => ({ path, content })),
  paths: Object.keys(mockFiles),
});

// ---- Imports ---------------------------------------------------------------------------

registerMock("web_import_goto", async ({ path, content, offset }) => callWasm((await wasm()).web_import_goto, { ...project(), path, content, offset }));
registerMock("web_import_complete", async ({ path, content, offset }) =>
  callWasm((await wasm()).web_import_complete, { ...project(), path, content, offset }),
);
registerMock("web_aliases", async () => callWasm((await wasm()).web_aliases, project()));

// ---- File Watchers ---------------------------------------------------------------------

registerMock("file_watchers_templates", async () => callWasm((await wasm()).file_watchers_templates, {}));
registerMock("file_watchers_validate", async ({ watcher, others }) => callWasm((await wasm()).file_watchers_validate, { watcher, others }));
registerMock("file_watchers_plan", async ({ paths, trigger }) => {
  const watchers = (mockSettingValue("tools.fileWatchers") as unknown[] | null) ?? [];
  if (!watchers.length) return [];
  if (mockSafeMode()) throw new Error("File Watchers is not available in Safe Mode. Trust the project to run it.");
  return callWasm((await wasm()).file_watchers_plan, { watchers, paths, trigger, root: root(), name: mockProjectName, files: mockFiles });
});
registerMock("file_watchers_problems", async ({ output, filters, cwd }) =>
  callWasm((await wasm()).file_watchers_problems, { output, filters, cwd, root: root() }),
);

/** `sass app.scss:app.css` in the demo: compile (a copy with a banner) or report an error. */
function sassRun(args: string[], cwd: string): { text: string; code: number } {
  const [input, output] = (args.find((a) => a.includes(":")) ?? "").split(":");
  const dir = cwd.startsWith(root()) ? cwd.slice(root().length).replace(/^\//, "") : "";
  const src = mockFiles[dir ? `${dir}/${input}` : input] ?? "";
  if (/\$[a-z]+;/.test(src) && !src.includes("@use")) {
    return { text: `Error: Undefined variable.\n  ╷\n2 │ ...\n  ╵\n  ${input} 2:10  root stylesheet\n`, code: 65 };
  }
  if (output) mockFiles[dir ? `${dir}/${output}` : output] = `/* Compiled by sass from ${input} */\n${src.replace(/@use[^\n]*\n/g, "")}`;
  return { text: `Compiled ${input} to ${output}.\n`, code: 0 };
}

/** Watcher programs the demo answers (commands built in the UI arrive without a configuration). */
const WATCHER_PROGRAMS = new Set(["sass", "lessc", "tsc", "babel", "prettier"]);
const watcherCommand = (r: RunRequest) => {
  const c = r.command;
  if (!c) return null;
  const program = c.program.split("/").pop() ?? "";
  return WATCHER_PROGRAMS.has(program) ? { ...c, base: program } : null;
};

registerMockRunHandler({
  commandLine: async (r: RunRequest) => watcherCommand(r),
  start: async (r: RunRequest, runId: string) => {
    const c = watcherCommand(r);
    if (!c) return null;
    const o = { program: c.program, cwd: c.cwd ?? "" };
    const args = c.args;
    const program = c.base;
    const commandLine = [o.program, ...args].join(" ");
    const { text, code } = program === "sass" ? sassRun(args, o.cwd ?? "") : { text: `${program} finished.\n`, code: 0 };
    const w = await wasm();
    const output = Number(w.run_output_new(JSON.stringify({ testRunner: false, root: root(), cwd: o.cwd ?? root() })));
    const event = `run://${runId}`;
    setTimeout(() => emitMock(event, { type: "started", pid: 4242, commandLine } satisfies RunEvent), 0);
    setTimeout(() => {
      const stream = code ? "stderr" : "stdout";
      emitMock(event, { type: "output", stream, text } satisfies RunEvent);
      const rendered: RunEvent[] = JSON.parse(w.run_output_handle(JSON.stringify({ handle: output, input: { type: "output", stream, text } })));
      rendered.forEach((e) => emitMock(event, e));
      emitMock(event, { type: "exited", code, signal: null } satisfies RunEvent);
      w.run_output_drop(JSON.stringify({ handle: output }));
    }, 250);
    return { runId, commandLine };
  },
});

// ---- OpenAPI ---------------------------------------------------------------------------

const isSpecCandidate = (p: string) => /\.(ya?ml|json)$/i.test(p) && !/(package|composer|tsconfig|nx|project)\.json$|lock/.test(p);

registerMock("openapi_endpoints", async () => {
  const w = await wasm();
  return Object.entries(mockFiles)
    .filter(([p, t]) => isSpecCandidate(p) && /(^|\n)\s*"?(openapi|swagger)"?\s*:/.test(t))
    .map(([path, content]) => callWasmSync(w.openapi_file, { path, content }))
    .filter(Boolean);
});
registerMock("openapi_preview", async ({ path, content }) => callWasm((await wasm()).openapi_preview, { path, content }));
registerMock("openapi_http_request", async ({ path, content, method, opPath }) =>
  callWasm((await wasm()).openapi_http_request, { path, content, method, opPath }),
);

// ---- GraphQL ---------------------------------------------------------------------------

const GQL = /\.(graphql|gql|graphqls)$/i;

async function schemaFiles(): Promise<[string, string][]> {
  const w = await wasm();
  return Object.entries(mockFiles).filter(([p, t]) => GQL.test(p) && callWasmSync<boolean>(w.graphql_is_schema, { content: t }));
}

async function config() {
  for (const name of [".graphqlrc.yml", ".graphqlrc", "graphql.config.json", "graphql.config.yml", "graphql.config.ts"]) {
    if (mockFiles[name] !== undefined) return callWasm((await wasm()).graphql_config, { path: name, content: mockFiles[name] });
  }
  return null;
}

registerMock("graphql_info", async () => {
  const schema = await schemaFiles();
  return { config: await config(), schemaFiles: schema.map(([p]) => p), types: await callWasm((await wasm()).graphql_types, { schema }) };
});
registerMock("graphql_validate", async ({ path, content }) => callWasm((await wasm()).graphql_validate, { schema: await schemaFiles(), path, content }));
registerMock("graphql_complete", async ({ content, offset }) => callWasm((await wasm()).graphql_complete, { schema: await schemaFiles(), content, offset }));
registerMock("graphql_introspect", async ({ url, target }) => {
  const file = String(target ?? "schema.graphql");
  mockFiles[file] = `# Introspected from ${url} (browser mode: canned schema)\ntype Query {\n  hello: String\n}\n`;
  return file;
});

// ---- Nx, Nuxt, Deno --------------------------------------------------------------------

registerMock("nx_workspace", async () => {
  const files = Object.fromEntries(Object.entries(mockFiles).filter(([p]) => /(^|\/)(nx|project|package)\.json$/.test(p) && !p.includes("node_modules/")));
  return callWasm((await wasm()).nx_workspace, { files });
});
registerMock("nuxt_project", async ({ root: dir }) => callWasm((await wasm()).nuxt_project, { root: dir ?? "", paths: Object.keys(mockFiles) }));
registerMock("deno_tasks", async () => {
  const text = mockFiles["deno.json"] ?? mockFiles["deno.jsonc"] ?? "";
  return text ? callWasm((await wasm()).deno_tasks, { text }) : [];
});

// ---- Live Edit and cURL ----------------------------------------------------------------

let liveSeq = 0;
registerMock("companion_live_edit", async ({ path, text }) => {
  if (mockSettingValue("liveEdit.enabled") === false) return null;
  const css = /\.css$/i.test(String(path));
  const reload = /\.(html?|[cm]?[jt]sx?|vue|svelte|astro|php|twig)$/i.test(String(path));
  if (!css && !reload) return null;
  return { seq: ++liveSeq, kind: css ? "css" : "reload", file: path, name: String(path).split("/").pop(), text: css ? text : undefined };
});

registerMock("scratches_http_from_curl", async ({ command }) => {
  const r = await callWasm<{ text?: string; error?: string }>((await wasm()).scratches_http_from_curl, { command });
  if (r.error) throw new Error(r.error);
  return r.text;
});

registerMock("scratches_http_curl", async ({ content, line }) => {
  const w = await wasm();
  const env = mockFiles["http-client.env.json"] ?? null;
  const { variables } = callWasmSync<{ variables: Record<string, string> }>(w.scratches_http_environments, {
    public: env,
    private: mockFiles["http-client.private.env.json"] ?? null,
    name: null,
  });
  const resolved = callWasmSync<{ request: unknown } | null>(w.scratches_http_request, {
    content,
    line,
    env: variables,
    globals: {},
    timestamp: Math.floor(Date.now() / 1000),
    seed: 7,
    base: "",
    files: mockFiles,
    debug: false,
  });
  if (!resolved) throw new Error("no request at the caret");
  return callWasmSync<string>(w.scratches_http_to_curl, { request: resolved.request });
});

registerMock("scratches_http_clear_tokens", () => null);
