/**
 * DotNetForge for web and cloud projects (`app/src-tauri/src/commands/dotnet_web.rs`, logic
 * in `packs/dotnetforge/lang-dotnet`: `aspnet`, `razor`, `efcore`, `aspire`, `docker`,
 * `watch`, `secrets`): ASP.NET Core endpoints and their `.http` requests, Blazor components,
 * EF Core, .NET Aspire AppHosts, run output (Aspire dashboard, Hot Reload), Dockerfiles and
 * user secrets. Everything is computed in Rust.
 */
import { invoke } from "./index";
import type { CommandLine, RunConfiguration } from "./run";

export interface EndpointParam {
  name: string;
  /** `path`, `query`, `body`, `header` or `form`. */
  kind: string;
  typeName: string;
}

export interface Endpoint {
  method: string;
  path: string;
  handler: string;
  /** `minimal`, `controller`, `blazor` or `razorPages`. */
  framework: string;
  file: string;
  line: number;
  name?: string | null;
  params?: EndpointParam[];
  authorized: boolean;
  project?: string | null;
}

export interface EndpointsModel {
  endpoints: Endpoint[];
  baseUrls: Record<string, string>;
}

export interface ComponentParameter {
  name: string;
  typeName: string;
  required: boolean;
  cascading: boolean;
  capturesUnmatched: boolean;
  file: string;
  line: number;
}

export interface RazorComponent {
  name: string;
  path: string;
  namespace?: string | null;
  routes?: string[];
  typeParams?: string[];
  parameters: ComponentParameter[];
  project?: string | null;
}

export interface EfProject {
  name: string;
  path: string;
  provider?: string | null;
  design: boolean;
  runnable: boolean;
  version?: string | null;
}

export interface DbContextInfo {
  name: string;
  file: string;
  line: number;
  base: string;
  sets: { name: string; entity: string }[];
  project?: string | null;
}

export interface Migration {
  id: string;
  name: string;
  context?: string | null;
  file?: string | null;
  line: number;
  applied?: boolean | null;
  project?: string | null;
}

export interface EfModel {
  projects: EfProject[];
  contexts: DbContextInfo[];
  migrations: Migration[];
  localTool: boolean;
}

export type EfAction = "addMigration" | "removeMigration" | "listMigrations" | "updateDatabase" | "dropDatabase" | "script" | "scaffold" | "listContexts" | "installTool";

export interface EfRequest {
  action: EfAction;
  project?: string;
  startupProject?: string;
  context?: string;
  name?: string;
  outputDir?: string;
  connection?: string;
  provider?: string;
  tables?: string[];
  force?: boolean;
  dataAnnotations?: boolean;
  configuration?: string;
  framework?: string;
  noBuild?: boolean;
}

export interface EfOutput {
  migrations: Migration[];
  contexts?: string[];
  error?: string | null;
  hint?: string | null;
  toolMissing: boolean;
}

export interface AspireResource {
  name: string;
  kind: string;
  detail?: string | null;
  project?: string | null;
  parent?: string | null;
  references?: string[];
  external: boolean;
  line: number;
}

export interface AppHost {
  name: string;
  path: string;
  program: string;
  resources: AspireResource[];
}

export type HotReloadState = "starting" | "watching" | "applying" | "applied" | "noChanges" | "rudeEdit" | "restarting" | "buildFailed" | "waiting";

export interface WatchEvent {
  state: HotReloadState;
  message: string;
  files?: string[];
  prompt: boolean;
}

export interface RunOutput {
  aspire: { dashboardUrl?: string | null; started: boolean };
  watch: WatchEvent[];
}

export interface GeneratedDocker {
  dockerfilePath: string;
  dockerfile: string;
  dockerignorePath: string;
  dockerignore: string;
  context: string;
  image: string;
  port: number;
  dockerfileExists: boolean;
  dockerignoreExists: boolean;
}

export interface UserSecrets {
  project: string;
  id?: string | null;
  path?: string | null;
  exists: boolean;
}

/** The scaffold providers of the EF Core dialog. */
export const SCAFFOLD_PROVIDERS = [
  "Npgsql.EntityFrameworkCore.PostgreSQL",
  "Microsoft.EntityFrameworkCore.SqlServer",
  "Microsoft.EntityFrameworkCore.Sqlite",
  "Pomelo.EntityFrameworkCore.MySql",
  "Oracle.EntityFrameworkCore",
];

export const dotnetWebApi = {
  endpoints: () => invoke<EndpointsModel>("dotnet_endpoints"),
  httpRequest: (endpoint: Endpoint) => invoke<string>("dotnet_http_request", { endpoint }),
  components: () => invoke<RazorComponent[]>("dotnet_razor_components"),
  efModel: () => invoke<EfModel>("dotnet_ef_model"),
  efCommand: (request: EfRequest) => invoke<CommandLine>("dotnet_ef_command", { request }),
  efOutput: (output: string) => invoke<EfOutput>("dotnet_ef_output", { output }),
  aspireHosts: () => invoke<AppHost[]>("dotnet_aspire_hosts"),
  runOutput: (text: string) => invoke<RunOutput>("dotnet_run_output", { text }),
  dockerfile: (project: string, port?: number) => invoke<GeneratedDocker>("dotnet_dockerfile", { project, port }),
  dockerBuild: (configuration: RunConfiguration) => invoke<CommandLine>("dotnet_docker_build", { configuration }),
  userSecrets: (project: string) => invoke<UserSecrets>("dotnet_user_secrets", { project }),
  initUserSecrets: (project: string) => invoke<UserSecrets>("dotnet_user_secrets_init", { project }),
  readUserSecrets: (id: string) => invoke<string>("dotnet_user_secrets_read", { id }),
  writeUserSecrets: (id: string, content: string) => invoke<void>("dotnet_user_secrets_write", { id, content }),
};
