/**
 * PyForge web frameworks (app/src-tauri/src/commands/python_web.rs, crate
 * packs/pyforge/frameworks): Django, Flask, FastAPI and Celery read from the sources, the
 * editor's template and URL name navigation, and the manage.py commands.
 */
import { invoke } from "./index";
import type { RunConfiguration } from "./run";

export interface WebLocation {
  path: string;
  line: number;
  character: number;
}

export interface WebRoute {
  framework: "django" | "flask" | "fastapi";
  /** Empty for any method (Django); `WS` for websockets. */
  methods: string[];
  path: string;
  name?: string;
  handler: string;
  location: WebLocation;
  handlerLocation?: WebLocation;
  /** The path with sample parameter values, for the HTTP client. */
  samplePath: string;
}

export interface DjangoApp {
  label: string;
  entry: string;
  dir?: string;
}

export interface DjangoInfo {
  manage: string;
  root: string;
  settingsModule?: string;
  settingsFile?: string;
  rootUrlconf?: string;
  apps: DjangoApp[];
  templateDirs: string[];
  appDirs: boolean;
  jinja2: boolean;
}

export interface ModelField {
  name: string;
  kind: string;
  target?: string;
  location: WebLocation;
}

export interface DjangoModel {
  app: string;
  name: string;
  bases: string[];
  location: WebLocation;
  fields: ModelField[];
  isAbstract: boolean;
}

export interface CeleryTask {
  name: string;
  function: string;
  decorator: string;
  bound: boolean;
  location: WebLocation;
}

export interface AppInfo {
  app?: string;
  appDir?: string;
  location?: WebLocation;
}

export interface TemplateRoot {
  dir: string;
  framework: string;
  origin: string;
}

export interface WebProject {
  frameworks: string[];
  django?: DjangoInfo;
  flask?: AppInfo;
  fastapi?: AppInfo;
  celery?: AppInfo;
  routes: WebRoute[];
  models: DjangoModel[];
  tasks: CeleryTask[];
  templateRoots: TemplateRoot[];
  templates: { name: string; path: string }[];
}

export interface WebCompletions {
  /** UTF-16 range of the string content the item replaces. */
  from: number;
  to: number;
  items: { label: string; kind: "template" | "url"; detail: string }[];
}

export interface WebGutterMark {
  line: number;
  kind: "template" | "route" | "view" | "task";
  tooltip: string;
  targets: WebLocation[];
}

export interface WebProblem {
  line: number;
  start: number;
  end: number;
  message: string;
  severity: "error" | "warning" | "info";
  code: string;
}

export interface WebFileMarks {
  gutter: WebGutterMark[];
  problems: WebProblem[];
}

export interface ManageCommand {
  name: string;
  app: string;
  description: string;
  location?: WebLocation;
}

export interface ManageCommands {
  commands: ManageCommand[];
  /** `manage.py` (help --commands) or `static` (built in plus the project's). */
  source: "manage.py" | "static";
  message?: string;
}

/** A web run configuration as the Python run that starts it. */
export interface WebRunInfo {
  target: "script" | "module";
  script: string;
  args: string[];
  env: Record<string, string>;
  workingDirectory?: string;
  port?: number;
  /** The page to open: the server's root, FastAPI's `/docs`. */
  url?: string;
}

export const pythonWebApi = {
  project: (refresh = false) => invoke<WebProject>("python_web_project", { refresh }),
  goto: (path: string, content: string, offset: number) => invoke<WebLocation[]>("python_web_goto", { path, content, offset }),
  complete: (path: string, content: string, offset: number) =>
    invoke<WebCompletions | null>("python_web_complete", { path, content, offset }),
  marks: (path: string, content: string) => invoke<WebFileMarks>("python_web_marks", { path, content }),
  manageCommands: (refresh = false) => invoke<ManageCommands>("python_manage_commands", { refresh }),
  runInfo: (configuration: RunConfiguration) => invoke<WebRunInfo>("python_web_run_info", { configuration }),
};
