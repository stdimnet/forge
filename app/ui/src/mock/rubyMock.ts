/**
 * Browser-mode RubyForge for the `pantry` demo (`?project=ruby`): the interpreters of this
 * "machine" (rbenv, asdf, the system Ruby), the bundle, Rake tasks, script run targets,
 * Bundler / gem / Rake / IRB command lines and RuboCop's offenses — computed by the Rust
 * crates through WebAssembly from the demo files — and a simulated rdbg session that stops
 * in `Inventory#total_cents`. Nothing runs and nothing is written.
 *
 * `?project=ruby&ruby-lsp=missing` starts without Ruby LSP (the install offer).
 */
import { emitMock, mockHandler, registerMock } from "@/ipc";
import type { BreakpointsSnapshot, DebugEvent, Frame, Property, SessionInfo } from "@/features/debug/api";
import type { AdapterInfo, LaunchRequest } from "@/features/debug/dapApi";
import type { RubyStatus } from "@/ipc/ruby";
import { callWasm, forgeWasm } from "@/wasm";
import { isRubyDemo, rubyDemoInfo, rubyDemoName } from "./demoRubyProject";
import { isRailsDemo } from "./demoRailsProject";
import { onMockDidOpen, setMockServerStatus } from "./lspMock";
import { RUBY_ROOT, RUBY_TOOLS, rubyContext } from "./rubyTestsMock";
import { mockSettingValue, setSetting } from "./settingsMock";
import { mockFiles } from "./workspaceMock";

const ROOT = `~/Projects/${rubyDemoName}`;
const call = async <T>(fn: string, input: unknown): Promise<T> => {
  const wasm = await forgeWasm();
  return callWasm<T>((wasm as unknown as Record<string, (json: string) => string>)[fn], input);
};

const RBENV = "/home/anna/.rbenv/versions";
const MANAGED = "/home/anna/.local/share/forge/langservers/ruby/3.3.6";
let lspInstalled = typeof location === "undefined" || new URLSearchParams(location.search).get("ruby-lsp") !== "missing";

const interpreter = (manager: string, version: string, path: string) => ({
  path,
  version,
  manager,
  name: `${manager}: ${version}`,
  binDir: path.replace(/\/ruby$/, ""),
});

const INTERPRETERS = [
  interpreter("rbenv", "3.2.6", `${RBENV}/3.2.6/bin/ruby`),
  interpreter("rbenv", "3.3.6", `${RBENV}/3.3.6/bin/ruby`),
  interpreter("asdf", "jruby-9.4.8.0", "/home/anna/.asdf/installs/ruby/jruby-9.4.8.0/bin/ruby"),
  interpreter("system", "3.0.2", "/usr/bin/ruby"),
];

async function status(): Promise<RubyStatus> {
  const setting = String(mockSettingValue("ruby.interpreter") ?? "");
  const request = await call<{ version: string; file: string } | null>("ruby_requested_version", { files: mockFiles });
  const picked = setting ? INTERPRETERS.find((i) => i.path === setting || i.name === setting) : undefined;
  const chosen = picked
    ? { interpreter: picked, reason: "setting" as const, request: request ?? undefined }
    : { interpreter: INTERPRETERS[1], reason: "versionFile" as const, request: request ?? undefined };
  const lsp = String(mockSettingValue("ruby.lsp.path") ?? "") || (lspInstalled ? `${MANAGED}/bin/ruby-lsp` : null);
  return {
    interpreters: INTERPRETERS,
    chosen,
    request: request ?? undefined,
    tools: { ...RUBY_TOOLS, ...(lspInstalled ? { "ruby-lsp": `${MANAGED}/bin/ruby-lsp` } : {}) },
    probe: { version: "3.3.6", userDir: "/home/anna/.local/share/gem/ruby/3.3.0", gemPath: [`${RBENV}/3.3.6/lib/ruby/gems/3.3.0`, "/home/anna/.local/share/gem/ruby/3.3.0"] },
    managers: ["asdf", "rbenv", "system"],
    setting,
    bundler: "Gemfile" in mockFiles,
    lsp: { path: lsp, managed: !!lsp && lsp.startsWith(MANAGED), installDir: MANAGED },
  };
}

const withCwd = (c: { program: string; args: string[]; cwd?: string; env?: Record<string, string> }) => ({ ...c, cwd: c.cwd ? `${RUBY_ROOT}/${c.cwd}` : RUBY_ROOT });

registerMock("ruby_status", () => status());
registerMock("ruby_interpreter_probe", ({ path }) => {
  const p = String(path);
  if (!p.endsWith("ruby")) throw new Error(`${p} is not a Ruby interpreter`);
  return interpreter("custom", "3.4.1", p);
});
registerMock("ruby_bundle", () =>
  mockFiles.Gemfile ? call("ruby_bundle", { gemfile: mockFiles.Gemfile, lock: mockFiles["Gemfile.lock"] ?? null }) : null,
);
registerMock("ruby_bundle_command", async ({ action, gem }) =>
  withCwd(await call("ruby_action_command", { action: `bundle:${action}`, arg: gem ?? null, context: rubyContext() })),
);
registerMock("ruby_gem_install_command", async ({ gem }) => ({
  ...(await call<{ program: string; args: string[] }>("ruby_action_command", { action: `gem:${gem}`, context: rubyContext() })),
  cwd: "/tmp",
}));
registerMock("ruby_lsp_install_command", () => ({
  program: RUBY_TOOLS.gem,
  args: ["install", "ruby-lsp", "--install-dir", MANAGED, "--bindir", `${MANAGED}/bin`, "--no-document", "--env-shebang"],
  cwd: "/tmp",
  env: {},
}));
registerMock("ruby_lsp_refresh", () => {
  lspInstalled = true;
  setMockServerStatus({ id: "ruby-lsp", name: "Ruby LSP", state: "ready" });
});

const PANTRY_RAKE_TASKS = `rake build                     # Build pantry-0.4.0.gem into the pkg directory
rake build:checksum            # Generate SHA512 checksum of pantry-0.4.0.gem into the checksums directory
rake clean                     # Remove any temporary products
rake clobber                   # Remove any generated files
rake db:reset                  # Empty the pantry
rake db:seed                   # Load the sample pantry
rake default                   #
rake install                   # Build and install pantry-0.4.0.gem into system gems
rake release[remote]           # Create tag v0.4.0 and build and push pantry-0.4.0.gem to rubygems.org
rake rubocop                   # Run RuboCop
rake rubocop:autocorrect       # Autocorrect RuboCop offenses (only when it's safe)
rake rubocop:autocorrect_all   # Autocorrect RuboCop offenses (safe and unsafe)
rake shopping_list[month]      # Print the shopping list for a month
rake spec                      # Run RSpec code examples
rake test                      # Run tests
`;

/** `bin/rails --tasks` of the Rails demo (a few of them). */
const RAILS_RAKE_TASKS = `rake about                              # List versions of all Rails frameworks and the environment
rake assets:precompile                 # Compile all the assets named in config.assets.precompile
rake db:create                         # Create the database from DATABASE_URL or config/database.yml for the current RAILS_ENV
rake db:drop                           # Drop the database from DATABASE_URL or config/database.yml for the current RAILS_ENV
rake db:migrate                        # Migrate the database (options: VERSION=x, VERBOSE=false, SCOPE=blog)
rake db:migrate:status                 # Display status of migrations
rake db:prepare                        # Run setup if database does not exist, or run migrations if it does
rake db:reset                          # Drop and recreate all databases from their schema for the current environment and load the seeds
rake db:rollback                       # Roll the schema back to the previous version (specify steps w/ STEP=n)
rake db:schema:load                    # Load a database schema file (either db/schema.rb or db/structure.sql, depending on \`ENV['SCHEMA_FORMAT']\` or \`config.active_record.schema_format\`) into the database
rake db:seed                           # Load the seed data from db/seeds.rb
rake log:clear                         # Truncate all/specified *.log files in log/ to zero bytes (specify which logs with LOGS=test,development)
rake routes                            # Lists all your defined routes
rake stimulus:manifest:update          # Update the Stimulus manifest
rake test                              # Run all tests in test folder except system ones
rake test:system                       # Run system tests only
rake tmp:clear                         # Clear cache, socket and screenshot files from tmp/ (narrow w/ tmp:cache:clear, tmp:sockets:clear, tmp:screenshots:clear)
`;
const RAKE_TASKS = isRailsDemo() ? RAILS_RAKE_TASKS : PANTRY_RAKE_TASKS;

registerMock("ruby_rake_tasks", async () => {
  await new Promise((r) => setTimeout(r, 300));
  const tasks = await call<{ name: string; file?: string; line?: number }[]>("ruby_rake_tasks", { output: RAKE_TASKS });
  // `rake --where` for the project's own tasks.
  const own: Record<string, [string, number]> = { "db:seed": ["Rakefile", 18], "db:reset": ["Rakefile", 23], shopping_list: ["lib/tasks/report.rake", 3], spec: ["Rakefile", 7], test: ["Rakefile", 9], rubocop: ["Rakefile", 14] };
  for (const t of tasks) if (own[t.name]) [t.file, t.line] = own[t.name];
  return { tasks, source: "rake" };
});
registerMock("ruby_rake_command", async ({ task, args }) =>
  withCwd(await call("ruby_action_command", { action: `rake:${task}`, arg: args ?? null, context: rubyContext() })),
);
registerMock("ruby_file_targets", async ({ path, content }) => {
  const line = await call<number | null>("ruby_script_entry", { text: content });
  if (line === null) return [];
  const name = String(path).split("/").pop();
  return [{ line, configuration: { name, type: "ruby", options: { script: path } } }];
});
registerMock("ruby_irb_command", async () => withCwd(await call("ruby_action_command", { action: "irb", context: rubyContext() })));
registerMock("ruby_debug_plan", () => ({ port: 40417, found: true, rdbg: `${RUBY_TOOLS.bundle} exec rdbg` }));

// ---- RuboCop -----------------------------------------------------------------------------------------

interface Offense {
  severity: string;
  message: string;
  cop_name: string;
  corrected: boolean;
  correctable: boolean;
  location: { start_line: number; start_column: number; last_line: number; last_column: number; length: number; line: number; column: number };
}

/** What RuboCop would report for the demo's text (single quotes, useless assignments, long lines). */
function offenses(text: string): Offense[] {
  const out: Offense[] = [];
  const at = (line: number, c0: number, c1: number) => ({ start_line: line, start_column: c0, last_line: line, last_column: c1, length: c1 - c0 + 1, line, column: c0 });
  text.split("\n").forEach((l, i) => {
    const line = i + 1;
    const assign = l.match(/^(\s*)(total) = 0\s*$/);
    if (assign && !text.includes(`${assign[2]} +=`))
      out.push({
        severity: "warning",
        message: "Lint/UselessAssignment: Useless assignment to variable - `total`.",
        cop_name: "Lint/UselessAssignment",
        corrected: false,
        correctable: true,
        location: at(line, assign[1].length + 1, assign[1].length + 5),
      });
    for (const m of l.matchAll(/'[^'"#\\]*'/g)) {
      if (l.trimStart().startsWith("#")) break;
      out.push({
        severity: "convention",
        message: "Style/StringLiterals: Prefer double-quoted strings unless you need single quotes to avoid extra backslashes for escaping.",
        cop_name: "Style/StringLiterals",
        corrected: false,
        correctable: true,
        location: at(line, m.index! + 1, m.index! + m[0].length),
      });
    }
    if (/' \+ |\.to_s \+ '/.test(l))
      out.push({
        severity: "convention",
        message: "Style/StringConcatenation: Prefer string interpolation to string concatenation.",
        cop_name: "Style/StringConcatenation",
        corrected: false,
        correctable: true,
        location: at(line, l.length - l.trimStart().length + 1, l.length),
      });
    if (l.length > 120)
      out.push({ severity: "convention", message: `Layout/LineLength: Line is too long. [${l.length}/120]`, cop_name: "Layout/LineLength", corrected: false, correctable: true, location: at(line, 121, l.length) });
  });
  return out;
}

registerMock("ruby_rubocop", async ({ path, text }) => {
  await new Promise((r) => setTimeout(r, 250));
  const p = String(path);
  const body = typeof text === "string" ? text : (mockFiles[p] ?? "");
  const json = JSON.stringify({ metadata: { rubocop_version: "1.66.1" }, files: [{ path: p, offenses: offenses(body) }], summary: {} });
  return call("ruby_rubocop_report", { json, root: RUBY_ROOT, path: p });
});

/** RuboCop's safe autocorrect of the demo's offenses. */
registerMock("ruby_format", ({ content }) => {
  if (mockSettingValue("ruby.formatter") === "lsp" || mockSettingValue("ruby.formatter") === "none") return null;
  return String(content)
    .replace(/'([^'"#\\]*)' \+ (\w+(?:\.\w+)*)\.to_s \+ '([^'"#\\]*)'/g, '"$1#{$2}$3"')
    .replace(/'([^'"#\\]*)'/g, '"$1"')
    .replace(/^(\s*)total = 0\n/m, "");
});

// ---- rdbg ---------------------------------------------------------------------------------------------

const INVENTORY = "lib/pantry/inventory.rb";

interface RubySession {
  info: SessionInfo;
  stopped: boolean;
}

const sessions = new Map<string, RubySession>();
let seq = 900;
const emit = (e: DebugEvent) => emitMock(`debug://${e.type}`, e);

function stop(s: RubySession) {
  s.stopped = true;
  s.info = { ...s.info, status: "paused", reason: "breakpoint", threadId: 1, location: { file: INVENTORY, fileUri: `${ROOT}/${INVENTORY}`, line: 23 } };
  emit({ type: "session", session: s.info });
}

function end(s: RubySession) {
  sessions.delete(s.info.id);
  emit({ type: "ended", sessionId: s.info.id });
}

function attach(request: LaunchRequest): SessionInfo {
  const id = `dap-${++seq}`;
  const info: SessionInfo = {
    id,
    ideKey: "",
    language: "rb",
    engine: "Ruby (rdbg)",
    fileUri: `${ROOT}/${INVENTORY}`,
    file: INVENTORY,
    peer: "127.0.0.1:40417",
    serverName: request.name ?? "rdbg",
    serverId: null,
    status: "running",
    location: null,
    reason: null,
    exception: null,
    adapter: "rdbg",
    features: {
      threads: true,
      readMemory: false,
      writeMemory: false,
      disassemble: false,
      registers: false,
      dataBreakpoints: false,
      functionBreakpoints: true,
      conditionalBreakpoints: true,
      hitConditionalBreakpoints: false,
      logPoints: false,
      instructionStepping: false,
      goto: false,
      restart: false,
      completions: true,
      setVariable: false,
      modules: false,
      loadedSources: false,
      attached: true,
      exceptionFilters: [],
    },
  };
  const s: RubySession = { info, stopped: false };
  sessions.set(id, s);
  emit({ type: "session", session: info });
  emit({ type: "output", sessionId: id, stream: "console", text: "Ruby REPL: You can run any Ruby expression here.\nNote that output to the STDOUT/ERR printed on the TERMINAL.\n" });
  const snap = mockHandler("debug_breakpoints")?.({}) as BreakpointsSnapshot | undefined;
  const hit = snap?.breakpoints.some((b) => b.enabled && b.file === INVENTORY && b.line === 23);
  setTimeout(() => (sessions.has(id) && hit ? stop(s) : end(s)), 600);
  return info;
}

const prop = (name: string, classname: string, value: string, children = 0, reference?: number): Property => ({
  name,
  fullname: name,
  type: "dap",
  classname,
  value,
  hasChildren: children > 0,
  numChildren: children,
  page: 0,
  pageSize: 0,
  children: [],
  reference,
});

const own = (args: Record<string, unknown>) => sessions.get(String(args.sessionId ?? ""));

function route(cmd: string, fn: (s: RubySession, args: Record<string, unknown>) => unknown) {
  const previous = mockHandler(cmd);
  registerMock(cmd, (args) => {
    const s = own(args);
    if (s) return fn(s, args);
    return previous ? previous(args) : undefined;
  });
}

/** The `rdbg` adapter as `commands::ruby::sync_debug` registers it. */
const ADAPTER: AdapterInfo = {
  type: "rdbg",
  label: "Ruby (rdbg)",
  languages: ["rb", "rake", "ru", "gemspec"],
  launch: false,
  attachProcess: false,
  attachPort: false,
  coreDump: false,
  command: RUBY_TOOLS.rdbg,
};

if (isRubyDemo()) {
  const previousAdapters = mockHandler("dap_adapters")!;
  registerMock("dap_adapters", async (args) => [...((await previousAdapters(args)) as AdapterInfo[]).filter((a) => a.type !== "rdbg"), ADAPTER]);
  const dapStart = mockHandler("dap_start");
  registerMock("dap_start", (args) => {
    const request = args.request as LaunchRequest;
    return request.adapter === "rdbg" ? attach(request) : dapStart?.(args);
  });
  for (const cmd of ["debug_status", "debug_sessions"]) {
    const base = mockHandler(cmd)!;
    registerMock(cmd, async (args) => {
      const list = (await base(args)) as SessionInfo[] | { sessions: SessionInfo[] };
      const mine = [...sessions.values()].map((s) => s.info);
      return Array.isArray(list) ? [...list, ...mine] : { ...list, sessions: [...list.sessions, ...mine] };
    });
  }
}

route("dap_threads", () => [{ id: 1, name: "#1 bin/restock:9:in '<main>'" }]);
route("debug_step", (s) => {
  s.info = { ...s.info, status: "running", location: null, reason: null };
  emit({ type: "session", session: s.info });
  setTimeout(() => sessions.has(s.info.id) && end(s), 200);
});
route("debug_pause", (s) => stop(s));
route("debug_stop", (s) => end(s));
route("debug_detach", (s) => end(s));
route("debug_stack", (s): Frame[] =>
  !s.stopped
    ? []
    : [
        { level: 0, file: INVENTORY, fileUri: `${ROOT}/${INVENTORY}`, line: 23, where: "block in Pantry::Inventory#total_cents", id: 1 },
        { level: 1, file: null, fileUri: `${RBENV}/3.3.6/lib/ruby/3.3.0/enumerator.rb`, line: 0, where: "Array#sum", id: 2 },
        { level: 2, file: INVENTORY, fileUri: `${ROOT}/${INVENTORY}`, line: 23, where: "Pantry::Inventory#total_cents", id: 3 },
        { level: 3, file: "bin/restock", fileUri: `${ROOT}/bin/restock`, line: 12, where: "<main>", id: 4 },
      ],
);
route("debug_contexts", () => [
  { id: 0, name: "Local variables" },
  { id: 1, name: "Global variables" },
]);
route("debug_variables", (_s, args) =>
  Number(args.context ?? 0) === 1
    ? [prop("$PROGRAM_NAME", "String", '"bin/restock"'), prop("$LOAD_PATH", "Array", "[...]", 12)]
    : [
        prop("%self", "Pantry::Inventory", "#<Pantry::Inventory:0x00007f3a1c2e8f10 @items=[...]>", 1, 9301),
        prop("item", "Pantry::Item", '#<struct Pantry::Item name="Coffee", quantity=1, minimum=2, price_cents=525>', 4, 9302),
      ],
);
route("debug_property", (_s, args) => {
  const ref = Number(args.reference);
  const children =
    ref === 9302
      ? [prop("name", "String", '"Coffee"'), prop("quantity", "Integer", "1"), prop("minimum", "Integer", "2"), prop("price_cents", "Integer", "525")]
      : [prop("@items", "Array", "[#<struct Pantry::Item name=\"Rice\", …>, #<struct Pantry::Item name=\"Coffee\", …>]", 2)];
  return { ...prop(String(args.fullname), "", "", children.length, ref), children };
});
route("debug_eval", (_s, args) => {
  const e = String(args.expression).trim();
  if (e === "item.missing") return prop(e, "Integer", "1");
  if (e === "item.price.format") return prop(e, "String", '"€5.25"');
  if (e === "items.size") return prop(e, "Integer", "2");
  throw new Error(`NameError: undefined local variable or method '${e}' for an instance of Pantry::Inventory`);
});

// ---- the demo project ----------------------------------------------------------------------------------

if (isRubyDemo()) {
  registerMock("project_detect", () => structuredClone(rubyDemoInfo));
  setSetting("project.announcedFrameworks", [`${ROOT}#ruby`], "user");
  onMockDidOpen((path) => {
    if (!/\.(rb|rake|gemspec|ru|erb)$|(^|\/)(Gemfile|Rakefile)$/.test(path)) return;
    if (lspInstalled) setMockServerStatus({ id: "ruby-lsp", name: "Ruby LSP", state: "ready" });
  });
}

// Screenshot scenarios can start sessions directly.
if (import.meta.env.DEV && typeof window !== "undefined") {
  (window as unknown as { __forgeRubyDebugMock?: unknown }).__forgeRubyDebugMock = { attach };
}
