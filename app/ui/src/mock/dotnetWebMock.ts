/**
 * Browser-mode DotNetForge for web and cloud projects (`?project=dotnet`): ASP.NET Core
 * endpoints, Blazor components, the EF Core model, Aspire AppHosts, Dockerfiles, run output
 * and `dotnet ef` / `docker` command lines, computed by the Rust crates through
 * WebAssembly from the demo files; canned `dotnet ef`, `docker` and AppHost output. User
 * secrets live in memory (never in the demo project), and `dotnet ef` changes only the
 * in-memory demo files.
 */
import { registerMock } from "@/ipc";
import type { CommandLine, RunConfiguration } from "@/ipc/run";
import type { Endpoint, EfRequest, GeneratedDocker } from "@/ipc/dotnetWeb";
import { forgeWasm } from "@/wasm";
import { DOTNET_ROOT, dotnetContext } from "./dotnetTestsMock";
import type { Step } from "./runMock";
import { mockFiles } from "./workspaceMock";

const call = async <T>(fn: string, input: unknown): Promise<T> => {
  const wasm = (await forgeWasm()) as unknown as Record<string, (json: string) => string>;
  const out = JSON.parse(wasm[fn]!(JSON.stringify(input))) as T & { error?: string };
  if (out && typeof out === "object" && !Array.isArray(out) && typeof out.error === "string" && Object.keys(out).length === 1) throw new Error(out.error);
  return out;
};
const rel = (cwd?: string) => (cwd ? (cwd.startsWith("/") ? cwd : `${DOTNET_ROOT}/${cwd}`) : DOTNET_ROOT);
const toCommandLine = (c: CommandLine & { cwd?: string }): CommandLine => ({ ...c, cwd: rel(c.cwd) });

/** The demo's project files and sources (bin/obj never exist in the demo). */
const SOURCES = /\.(sln|slnx|csproj|fsproj|vbproj|props|cs|razor|cshtml)$|(^|\/)(global|launchSettings|dotnet-tools)\.json$/;
const files = () => Object.fromEntries(Object.entries(mockFiles).filter(([p]) => SOURCES.test(p)));

registerMock("dotnet_endpoints", () => call("dotnet_endpoints", { files: files() }));
registerMock("dotnet_http_request", ({ endpoint }) => call("dotnet_http_request", { files: files(), endpoint: endpoint as Endpoint }));
registerMock("dotnet_razor_components", () => call("dotnet_razor_components", { files: files() }));
registerMock("dotnet_ef_model", () => call("dotnet_ef_model", { files: files() }));
registerMock("dotnet_ef_command", async ({ request }) => toCommandLine(await call<CommandLine>("dotnet_ef_command", { request: request as EfRequest, context: await dotnetContext(mockFiles) })));
registerMock("dotnet_ef_output", ({ output }) => call("dotnet_ef_output", { text: String(output ?? "") }));
registerMock("dotnet_aspire_hosts", () => call("dotnet_aspire_hosts", { files: files() }));
registerMock("dotnet_run_output", ({ text }) => call("dotnet_run_output", { text: String(text ?? "") }));
registerMock("dotnet_dockerfile", async ({ project, port }) => {
  const g = await call<GeneratedDocker>("dotnet_dockerfile", { files: { ...files(), ...rootFiles() }, project, port });
  return { ...g, dockerfileExists: g.dockerfilePath in mockFiles, dockerignoreExists: g.dockerignorePath in mockFiles };
});
registerMock("dotnet_docker_build", async ({ configuration }) => {
  const c = await call<{ build: CommandLine }>("dotnet_web_run_command", { configuration: configuration as RunConfiguration, context: {} });
  return toCommandLine(c.build);
});

const rootFiles = () => Object.fromEntries(Object.keys(mockFiles).filter((p) => !p.includes("/")).map((p) => [p, ""]));

/** `dotnet-aspire` / `dotnet-docker` configurations: their process (`runMock.ts`). */
export async function dotnetWebCommandLine(cfg: RunConfiguration): Promise<CommandLine> {
  const out = await call<CommandLine & { run?: CommandLine; cwd?: string }>("dotnet_web_run_command", { configuration: cfg, context: await dotnetContext(mockFiles) });
  const c = out.run ?? out;
  return { ...toCommandLine(c), env: { ...(c.env ?? {}), ...(cfg.env ?? {}) } };
}

// ---- User secrets (in memory, outside the demo project) ----------------------------------------------

const secrets = new Map<string, string>([["b7c1e7a2-6f0e-4f53-9d55-3f0b1e5d9a11", '{\n  "ConnectionStrings:bookstore": "Host=localhost;Database=bookstore;Username=app;Password=dev-only",\n  "Stripe:ApiKey": "sk_test_demo"\n}\n']]);
const secretsPath = (id: string) => `/home/anna/.microsoft/usersecrets/${id}/secrets.json`;

async function projectOf(path: string) {
  const ws = await call<{ projects: { path: string; properties: Record<string, string> }[] }>("dotnet_load_workspace", {
    files: files(),
    solution: "",
    configuration: "Debug",
  });
  return ws.projects.find((p) => p.path === path);
}

registerMock("dotnet_user_secrets", async ({ project }) => {
  const p = await projectOf(String(project));
  if (!p) throw new Error(`No project '${project}'`);
  const id = p.properties.UserSecretsId;
  return { project: p.path, id: id ?? null, path: id ? secretsPath(id) : null, exists: !!id && secrets.has(id) };
});
registerMock("dotnet_user_secrets_init", async ({ project }) => {
  const path = String(project);
  const p = await projectOf(path);
  if (!p) throw new Error(`No project '${path}'`);
  let id = p.properties.UserSecretsId;
  if (!id) {
    id = "5d0f3c2b-8a41-4e6d-9b7c-1e2f3a4b5c6d";
    mockFiles[path] = await call<string>("dotnet_add_user_secrets_id", { projectXml: mockFiles[path] ?? "", id });
  }
  if (!secrets.has(id)) secrets.set(id, "{\n}\n");
  return { project: path, id, path: secretsPath(id), exists: true };
});
registerMock("dotnet_user_secrets_read", ({ id }) => secrets.get(String(id)) ?? "{\n}\n");
registerMock("dotnet_user_secrets_write", ({ id, content }) => void secrets.set(String(id), String(content)));

// ---- Canned output ------------------------------------------------------------------------------------

const lines = (text: string, delay = 60): Step[] => text.split(/(?<=\n)/).map((l): Step => [delay, l]);
const BUILD = "Build started...\nBuild succeeded.\n";
const MIGRATIONS_DIR = "src/Bookstore.Api/Data/Migrations";
/** Migrations applied to the demo database. */
const applied = new Set(["20241001120000_InitialCreate", "20241005093000_AddAuthors"]);
const migrationIds = () =>
  Object.keys(mockFiles)
    .map((p) => new RegExp(`^${MIGRATIONS_DIR}/(\\d{14}_\\w+)\\.Designer\\.cs$`).exec(p)?.[1])
    .filter((x): x is string => !!x)
    .sort();
const designer = (id: string) =>
  `// <auto-generated />\nusing Bookstore.Api.Data;\nusing Microsoft.EntityFrameworkCore.Infrastructure;\nusing Microsoft.EntityFrameworkCore.Migrations;\n\n#nullable disable\n\nnamespace Bookstore.Api.Data.Migrations\n{\n    [DbContext(typeof(BookstoreContext))]\n    [Migration("${id}")]\n    partial class ${id.split("_").slice(1).join("_")}\n    {\n    }\n}\n`;

function efSteps(args: string[]): Step[] {
  const [, group, verb, arg] = args;
  const prefix = args.includes("--prefix-output");
  const p = (tag: string, l: string) => (prefix ? `${tag}: ${l}` : l);
  if (group === "migrations" && verb === "list") {
    const json = JSON.stringify(
      migrationIds().map((id) => ({ id, name: id.split("_").slice(1).join("_"), safeName: id.split("_").slice(1).join("_"), applied: applied.has(id) })),
      null,
      2,
    );
    return [...lines(`${p("info", "Build started...")}\n${p("info", "Build succeeded.")}\n`, 500), ...lines(json.split("\n").map((l) => p("data", l)).join("\n") + "\n", 20), [10, { code: 0 }]];
  }
  if (group === "migrations" && verb === "add" && arg) {
    const id = `20241028${String(90000 + migrationIds().length).padStart(6, "0")}_${arg}`;
    mockFiles[`${MIGRATIONS_DIR}/${id}.Designer.cs`] = designer(id);
    return [...lines(`${BUILD}Done. To undo this action, use 'ef migrations remove'\n`, 400), [10, { code: 0 }]];
  }
  if (group === "migrations" && verb === "remove") {
    const last = migrationIds().at(-1);
    if (last && applied.has(last) && !args.includes("--force"))
      return [...lines(`${BUILD}The migration '${last}' has already been applied to the database. Revert it and try again.\n`, 300), [10, { code: 1 }]];
    if (last) {
      delete mockFiles[`${MIGRATIONS_DIR}/${last}.Designer.cs`];
      applied.delete(last);
    }
    return [...lines(`${BUILD}Removing migration '${last ?? ""}'.\nReverting the model snapshot.\nDone.\n`, 300), [10, { code: 0 }]];
  }
  if (group === "database" && verb === "update") {
    const target = arg && !arg.startsWith("-") ? arg : null;
    const ids = migrationIds();
    const upTo = target === "0" ? -1 : target ? ids.findIndex((id) => id.endsWith(`_${target}`)) : ids.length - 1;
    const out: string[] = [];
    ids.forEach((id, i) => {
      if (i <= upTo && !applied.has(id)) {
        applied.add(id);
        out.push(`Applying migration '${id}'.`);
      } else if (i > upTo && applied.has(id)) {
        applied.delete(id);
        out.push(`Reverting migration '${id}'.`);
      }
    });
    return [...lines(`${BUILD}${out.join("\n")}${out.length ? "\n" : "No migrations were applied. The database is already up to date.\n"}Done.\n`, 300), [10, { code: 0 }]];
  }
  if (group === "dbcontext" && verb === "scaffold") return [...lines(`${BUILD}To protect potentially sensitive information in your connection string, you should move it out of source code.\n`, 400), [10, { code: 0 }]];
  return [...lines(BUILD, 300), [10, { code: 0 }]];
}

const DOCKER_BUILD = `#0 building with "default" instance using docker driver

#1 [internal] load build definition from Dockerfile
#1 transferring dockerfile: 1.21kB done
#2 [internal] load metadata for mcr.microsoft.com/dotnet/sdk:8.0
#3 [internal] load metadata for mcr.microsoft.com/dotnet/aspnet:8.0
#4 [build 4/8] RUN dotnet restore "src/Bookstore.Api/Bookstore.Api.csproj"
#4 2.913   Restored /src/src/Bookstore.Api/Bookstore.Api.csproj (in 2.41 sec).
#5 [build 7/8] RUN dotnet build "Bookstore.Api.csproj" -c Release -o /app/build
#5 9.402 Build succeeded.
#6 [publish 1/1] RUN dotnet publish "Bookstore.Api.csproj" -c Release -o /app/publish /p:UseAppHost=false
#7 exporting to image
#7 naming to docker.io/library/bookstore.api:latest done
`;

/** Output of `dotnet ef`, `dotnet tool install` and `docker build` command lines (`dotnet-command`). */
export function dotnetWebSteps(cmd: { program?: string; args?: string[] }): Step[] | null {
  const program = (cmd.program ?? "").split("/").pop() ?? "";
  const args = cmd.args ?? [];
  if (program === "docker" && args[0] === "build") return [...lines(DOCKER_BUILD, 180), [10, { code: 0 }]];
  if (args[0] === "ef") return efSteps(args);
  if (args[0] === "tool" && args[1] === "install") return [...lines("You can invoke the tool using the following command: dotnet-ef\nTool 'dotnet-ef' (version '8.0.11') was successfully installed.\n", 500), [10, { code: 0 }]];
  return null;
}

/** A running AppHost (`dotnet-aspire`). */
export function aspireRunSteps(): Step[] {
  return [
    [500, "Building...\n"],
    [900, "info: Aspire.Hosting.DistributedApplication[0]\n"],
    [10, "      Aspire version: 9.0.0+01ed51919f8df692ececce51048a140615dc759d\n"],
    [10, "info: Aspire.Hosting.DistributedApplication[0]\n"],
    [10, "      Distributed application starting.\n"],
    [10, "info: Aspire.Hosting.DistributedApplication[0]\n"],
    [10, `      Application host directory is: ${DOTNET_ROOT}/src/Bookstore.AppHost\n`],
    [300, "info: Aspire.Hosting.DistributedApplication[0]\n"],
    [10, "      Now listening on: https://localhost:17037\n"],
    [10, "info: Aspire.Hosting.DistributedApplication[0]\n"],
    [10, "      Login to the dashboard at https://localhost:17037/login?t=8f3a1c2d9e7b4a60\n"],
    [600, "info: Aspire.Hosting.DistributedApplication[0]\n"],
    [10, "      Distributed application started. Press Ctrl+C to shut down.\n"],
  ];
}

/** A container of a `.NET Docker` configuration. */
export function dockerRunSteps(): Step[] {
  return [
    [400, "info: Microsoft.Hosting.Lifetime[14]\n"],
    [10, "      Now listening on: http://[::]:8080\n"],
    [10, "info: Microsoft.Hosting.Lifetime[0]\n"],
    [10, "      Application started. Press Ctrl+C to shut down.\n"],
    [10, "info: Microsoft.Hosting.Lifetime[0]\n"],
    [10, "      Hosting environment: Development\n"],
  ];
}
