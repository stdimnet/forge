/**
 * Browser-mode Ruby on Rails for the `blog` demo (`?project=rails`): the `rails_*` commands
 * answered by the Rust crate (`rubyforge-fw-rails`) through WebAssembly from the demo files —
 * routes, models, locales, Stimulus, database.yml, navigation, completion, gutter marks,
 * inspections and folds — plus a canned `bin/rails generate` list, generator and console
 * command lines and data sources added to the mock Database tool window. Nothing runs and
 * nothing is written. Other projects get an empty, non-Rails answer.
 */
import { mockHandler, registerMock } from "@/ipc";
import type { CommandLine } from "@/ipc/run";
import type { RailsDatabase, RailsGenerator, RailsProject } from "@/ipc/rails";
import { callWasm, forgeWasm } from "@/wasm";
import { isRailsDemo } from "./demoRailsProject";
import { RUBY_ROOT, rubyContext } from "./rubyTestsMock";
import { mockSettingValue } from "./settingsMock";
import { mockFiles } from "./workspaceMock";

const call = async <T>(fn: string, input: unknown): Promise<T> => {
  const wasm = await forgeWasm();
  return callWasm<T>((wasm as unknown as Record<string, (json: string) => string>)[fn], input);
};

const EMPTY: RailsProject = {
  rails: false,
  binstub: false,
  routes: [],
  routesSource: "",
  controllers: [],
  models: [],
  tables: [],
  defaultLocale: "en",
  locales: [],
  translations: 0,
  stimulus: [],
  databases: [],
  views: 0,
};

/** `DB_PASSWORD` as if set in the shell, so the development database has a password. */
const ENV = { DB_PASSWORD: "blog-dev" };

const request = (extra: Record<string, unknown> = {}) => ({ files: mockFiles, options: { env: ENV }, ...extra });

/** What `bin/rails generate` prints in the demo (with the gems' generators). */
const GENERATE_OUTPUT = `Usage:
  bin/rails generate GENERATOR [args] [options]

Please choose a generator below.

Rails:
  application_record
  authentication
  benchmark
  channel
  controller
  generator
  helper
  integration_test
  jbuilder
  job
  mailbox
  mailer
  migration
  model
  resource
  scaffold
  scaffold_controller
  script
  system_test
  task

ActiveRecord:
  active_record:application_record
  active_record:multi_db

Stimulus:
  stimulus

TestUnit:
  test_unit:channel
  test_unit:generator
  test_unit:install
  test_unit:mailbox
  test_unit:plugin
`;

const withCwd = (c: { program: string; args: string[]; cwd?: string; env?: Record<string, string> }): CommandLine => ({
  program: c.program,
  args: c.args,
  cwd: c.cwd ? `${RUBY_ROOT}/${c.cwd}` : RUBY_ROOT,
  env: c.env ?? {},
});

registerMock("rails_project", async () => {
  if (!isRailsDemo()) return structuredClone(EMPTY);
  const info = await call<RailsProject>("rails_project", request());
  if (mockSettingValue("rails.routesFromRails") === true) info.routesError = "bin/rails routes is not available in browser mode: routes are read from config/routes.rb";
  return info;
});
registerMock("rails_goto", async ({ path, content, offset }) => (isRailsDemo() ? call("rails_goto", request({ path, content, offset })) : []));
registerMock("rails_complete", async ({ path, content, offset }) => (isRailsDemo() ? call("rails_complete", request({ path, content, offset })) : null));
registerMock("rails_marks", async ({ path, content }) =>
  isRailsDemo() ? call("rails_marks", request({ path, content })) : { gutter: [], problems: [], folds: [] },
);
registerMock("rails_generators", async () => {
  const files = Object.keys(mockFiles);
  const fromRails = isRailsDemo() && mockSettingValue("rails.generatorsFromRails") !== false;
  const generators = await call<RailsGenerator[]>("rails_generators", fromRails ? { files, output: GENERATE_OUTPUT } : { files });
  return { generators, source: fromRails ? "rails" : "static" };
});
registerMock("rails_generate_command", async ({ generator, arguments: args, options, destroy }) =>
  withCwd(
    await call("rails_command", {
      kind: destroy ? "destroy" : "generate",
      context: rubyContext(),
      binstub: true,
      generator,
      arguments: args,
      options,
    }),
  ),
);
registerMock("rails_console_command", async ({ kind }) =>
  withCwd(
    await call("rails_command", {
      kind: kind === "dbconsole" ? "dbconsole" : "console",
      context: rubyContext(),
      binstub: true,
      environment: String(mockSettingValue("rails.console.environment") ?? ""),
      sandbox: mockSettingValue("rails.console.sandbox") === true,
    }),
  ),
);

/** Add the databases of database.yml to the mock Database tool window (`db_data_source_save`). */
registerMock("rails_add_data_sources", async ({ environments }) => {
  if (!isRailsDemo()) return [];
  const info = await call<RailsProject>("rails_project", request());
  const save = mockHandler("db_data_source_save");
  const list = mockHandler("db_data_sources_list");
  if (!save) return [];
  const existing = ((await list?.({})) ?? []) as { driver: string; host: string; database: string; file: string }[];
  const wanted = (environments as string[] | undefined) ?? [];
  const added = [];
  for (const d of info.databases as RailsDatabase[]) {
    if (!d.driver || (wanted.length && !wanted.includes(d.environment))) continue;
    const host = d.host || "localhost";
    if (existing.some((e) => e.driver === d.driver && e.database === d.database && (e.file ?? "") === d.file)) continue;
    const name = `${d.driver === "sqlite" ? d.file.split("/").pop() : `${d.database}@${host === "127.0.0.1" ? "localhost" : host}`} (${d.environment})`;
    added.push(
      await save({
        source: {
          id: "",
          name,
          driver: d.driver,
          host: d.driver === "sqlite" ? "" : host,
          port: d.port ?? null,
          database: d.database,
          user: d.username,
          file: d.file,
          comment: `From config/database.yml (${d.environment})`,
          options: {
            readOnly: false,
            autoCommit: true,
            sslMode: "prefer",
            schemas: [],
            connectTimeoutSecs: 10,
            initSql: "",
            savePassword: true,
            pageSize: null,
            color: d.environment.startsWith("production") ? "red" : null,
          },
          scope: "project",
        },
        password: d.hasPassword ? ENV.DB_PASSWORD : null,
      }),
    );
  }
  return added;
});
