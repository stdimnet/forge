import { describe, expect, it } from "vitest";
import type { AppHost, EfModel, EfOutput, EndpointsModel, GeneratedDocker, RazorComponent, RunOutput, UserSecrets } from "@/ipc/dotnetWeb";
import type { CommandLine } from "@/ipc/run";

// The .NET demo is chosen by the page URL, before the mocks load.
history.replaceState(null, "", "/?project=dotnet");
const { invoke } = await import("@/ipc");
await import("./settingsMock");
await import("./workspaceMock");
await import("./dotnetMock");
await import("./dotnetWebMock");
const { dotnetWebSteps } = await import("./dotnetWebMock");
const { mockFiles } = await import("./workspaceMock");

describe("DotNetForge web mock (Rust crates through WebAssembly)", () => {
  it("finds minimal API, controller and Blazor endpoints", async () => {
    const m = await invoke<EndpointsModel>("dotnet_endpoints");
    const rows = m.endpoints.map((e) => `${e.method} ${e.path}`);
    expect(rows).toContain("GET /api/authors/{id:int}");
    expect(rows).toContain("POST /api/Orders");
    expect(rows).toContain("GET /shelf/{Genre}");
    expect(m.endpoints.every((e) => e.project === "src/Bookstore.Api/Bookstore.Api.csproj")).toBe(true);
    const get = m.endpoints.find((e) => e.path === "/api/authors/{id:int}")!;
    const http = await invoke<string>("dotnet_http_request", { endpoint: get });
    expect(http).toBe("@id = 1\n\n### GetAuthor\nGET http://localhost:5180/api/authors/{{id}}\nAccept: application/json\n");
  });

  it("reads Blazor components with their parameters", async () => {
    const c = await invoke<RazorComponent[]>("dotnet_razor_components");
    const rating = c.find((x) => x.name === "Rating")!;
    expect(rating.namespace).toBe("Bookstore.Api.Components");
    expect(rating.parameters.map((p) => `${p.name}${p.required ? "!" : ""}`)).toEqual(["Value!", "Max", "ValueChanged"]);
  });

  it("runs EF Core migrations on the demo files", async () => {
    const model = await invoke<EfModel>("dotnet_ef_model");
    expect(model.localTool).toBe(true);
    expect(model.projects[0]).toMatchObject({ name: "Bookstore.Api", provider: "Npgsql.EntityFrameworkCore.PostgreSQL", design: true });
    expect(model.contexts[0]!.sets.map((s) => s.name)).toEqual(["Books", "Authors", "Reviews"]);
    expect(model.migrations.map((m) => m.name)).toEqual(["InitialCreate", "AddAuthors", "AddReviews"]);
    const list = await invoke<CommandLine>("dotnet_ef_command", { request: { action: "listMigrations", project: "src/Bookstore.Api/Bookstore.Api.csproj" } });
    expect(list.args.slice(0, 4)).toEqual(["ef", "migrations", "list", "--json"]);
    const out = dotnetWebSteps(list)!.map((s) => (typeof s[1] === "string" ? s[1] : "")).join("");
    const parsed = await invoke<EfOutput>("dotnet_ef_output", { output: out });
    expect(parsed.migrations.map((m) => m.applied)).toEqual([true, true, false]);
    const add = await invoke<CommandLine>("dotnet_ef_command", { request: { action: "addMigration", name: "AddWishlist", project: "src/Bookstore.Api/Bookstore.Api.csproj" } });
    dotnetWebSteps(add);
    expect((await invoke<EfModel>("dotnet_ef_model")).migrations.at(-1)?.name).toBe("AddWishlist");
  });

  it("knows the Aspire AppHost and its dashboard", async () => {
    const hosts = await invoke<AppHost[]>("dotnet_aspire_hosts");
    expect(hosts[0]!.name).toBe("Bookstore.AppHost");
    const api = hosts[0]!.resources.find((r) => r.name === "api")!;
    expect(api).toMatchObject({ kind: "project", project: "src/Bookstore.Api/Bookstore.Api.csproj", references: ["bookstore", "cache", "postgres"] });
    const r = await invoke<RunOutput>("dotnet_run_output", { text: "      Login to the dashboard at https://localhost:17037/login?t=abc\ndotnet watch 🔥 Hot reload of changes succeeded.\n" });
    expect(r.aspire.dashboardUrl).toBe("https://localhost:17037/login?t=abc");
    expect(r.watch[0]!.state).toBe("applied");
  });

  it("generates a Dockerfile and keeps user secrets out of the project", async () => {
    const g = await invoke<GeneratedDocker>("dotnet_dockerfile", { project: "src/Bookstore.Api/Bookstore.Api.csproj" });
    expect(g.dockerfilePath).toBe("src/Bookstore.Api/Dockerfile");
    expect(g.dockerfile).toContain('COPY ["src/Bookstore.Core/Bookstore.Core.csproj", "src/Bookstore.Core/"]');
    expect(g.dockerfileExists).toBe(false);
    const s = await invoke<UserSecrets>("dotnet_user_secrets", { project: "src/Bookstore.Core/Bookstore.Core.csproj" });
    expect(s.id).toBeNull();
    const init = await invoke<UserSecrets>("dotnet_user_secrets_init", { project: "src/Bookstore.Core/Bookstore.Core.csproj" });
    expect(init.path).toBe(`/home/anna/.microsoft/usersecrets/${init.id}/secrets.json`);
    expect(mockFiles["src/Bookstore.Core/Bookstore.Core.csproj"]).toContain(`<UserSecretsId>${init.id}</UserSecretsId>`);
    expect(Object.keys(mockFiles).some((p) => p.endsWith("secrets.json"))).toBe(false);
    await invoke("dotnet_user_secrets_write", { id: init.id, content: '{ "a": 1 }' });
    expect(await invoke<string>("dotnet_user_secrets_read", { id: init.id })).toBe('{ "a": 1 }');
  });
});
