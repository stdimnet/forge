/**
 * Ruby on Rails (`app/src-tauri/src/commands/rails.rs`, logic in `packs/rubyforge/fw-rails`):
 * the project model (routes, controllers, models with their columns, locales, Stimulus
 * controllers, database.yml), the editor's navigation, completion, gutter marks, inspections
 * and folds, generators and the command lines of generators and consoles. Everything is
 * computed in Rust.
 */
import { invoke } from "./index";
import type { CommandLine } from "./run";
import type { DataSourceInfo } from "./db";

export interface RailsLocation {
  path: string;
  line: number;
  character: number;
}

export interface RailsRoute {
  /** Empty for any method (mounts, `via: :all`). */
  methods: string[];
  /** `/articles/:id` without `(.:format)`. */
  path: string;
  /** The helper prefix: `article` → `article_path`. */
  name?: string;
  /** `admin/users`; an application class for mounts. */
  controller: string;
  action: string;
  kind: "route" | "mount" | "redirect";
  location: RailsLocation;
  actionLocation?: RailsLocation;
  samplePath: string;
}

export interface RailsNamed {
  name: string;
  location: RailsLocation;
}

export interface RailsController {
  name: string;
  class: string;
  kind: "controller" | "mailer";
  path: string;
  location: RailsLocation;
  actions: RailsNamed[];
  layout?: string;
}

export interface RailsColumn {
  name: string;
  type: string;
  notNull?: boolean;
  location: RailsLocation;
}

export interface RailsAssociation {
  kind: string;
  name: string;
  className: string;
  location: RailsLocation;
}

export interface RailsModel {
  name: string;
  superclass: string;
  path: string;
  location: RailsLocation;
  table: string;
  associations: RailsAssociation[];
  scopes: RailsNamed[];
  columns: RailsColumn[];
}

export interface RailsTable {
  name: string;
  columns: RailsColumn[];
  location: RailsLocation;
  migrations: RailsLocation[];
}

export interface StimulusController {
  identifier: string;
  path: string;
  location: RailsLocation;
  targets: RailsNamed[];
  values: RailsNamed[];
  classes: RailsNamed[];
  outlets: RailsNamed[];
  methods: RailsNamed[];
}

export interface RailsDatabase {
  /** `development`, `production/cache`. */
  environment: string;
  adapter: string;
  /** Forge's driver; missing when the Database tool window has none for the adapter. */
  driver?: string;
  host: string;
  port?: number;
  database: string;
  username: string;
  file: string;
  hasPassword: boolean;
  location: RailsLocation;
}

export interface RailsProject {
  rails: boolean;
  version?: string;
  appName?: string;
  binstub: boolean;
  routes: RailsRoute[];
  /** `routes.rb` (read statically) or `rails routes`. */
  routesSource: string;
  routesError?: string;
  controllers: RailsController[];
  models: RailsModel[];
  tables: RailsTable[];
  defaultLocale: string;
  locales: string[];
  translations: number;
  stimulus: StimulusController[];
  databases: RailsDatabase[];
  views: number;
}

export interface RailsCompletionItem {
  label: string;
  kind: "route" | "partial" | "template" | "i18n" | "attribute" | "association" | "scope" | "stimulus" | "action" | "target" | "turbo" | "frame";
  detail: string;
  apply?: string;
}

export interface RailsCompletions {
  /** UTF-16 range the items replace. */
  from: number;
  to: number;
  items: RailsCompletionItem[];
}

export interface RailsGutterMark {
  line: number;
  kind: "view" | "route" | "controller" | "partial" | "table" | "model" | "action" | "stimulus";
  tooltip: string;
  targets: RailsLocation[];
}

export interface RailsProblem {
  line: number;
  start: number;
  end: number;
  message: string;
  severity: "error" | "warning" | "info";
  code: "RailsI18nMissing" | "RailsMissingView" | "RailsStimulus";
}

export interface RailsFold {
  from: number;
  to: number;
  text: string;
  key: string;
}

export interface RailsFileMarks {
  gutter: RailsGutterMark[];
  problems: RailsProblem[];
  folds: RailsFold[];
}

export interface RailsGenerator {
  name: string;
  group: string;
  usage: string;
  description: string;
}

export interface RailsGenerators {
  generators: RailsGenerator[];
  /** `rails` (bin/rails generate) or `static` (Rails' own and lib/generators). */
  source: "rails" | "static";
  message?: string;
}

export const railsApi = {
  project: (refresh = false) => invoke<RailsProject>("rails_project", { refresh }),
  goto: (path: string, content: string, offset: number) => invoke<RailsLocation[]>("rails_goto", { path, content, offset }),
  complete: (path: string, content: string, offset: number) => invoke<RailsCompletions | null>("rails_complete", { path, content, offset }),
  marks: (path: string, content: string) => invoke<RailsFileMarks>("rails_marks", { path, content }),
  generators: (refresh = false) => invoke<RailsGenerators>("rails_generators", { refresh }),
  generateCommand: (generator: string, args: string, options: string[], destroy: boolean) =>
    invoke<CommandLine>("rails_generate_command", { generator, arguments: args, options, destroy }),
  consoleCommand: (kind: "console" | "dbconsole") => invoke<CommandLine>("rails_console_command", { kind }),
  addDataSources: (environments: string[] = []) => invoke<DataSourceInfo[]>("rails_add_data_sources", { environments }),
};
