import { beforeAll, describe, expect, it } from "vitest";
import { fileWatchersApi, graphqlApi, httpExtrasApi, importsApi, nuxtApi, nxApi, openApiApi, watcherFromObject } from "@/ipc/webTooling";
import { forgeWasm } from "@/wasm";
import "./settingsMock";
import { setSetting } from "./settingsMock";
import { mockFiles } from "./workspaceMock";
import "./webToolingMock";
import { webToolingFiles, webToolingWatchers } from "./demoWebTooling";

describe("web tooling mock (Rust through WebAssembly)", () => {
  beforeAll(async () => {
    await forgeWasm();
    // The web tooling demo next to the Laravel demo the tests load.
    for (const [path, content] of Object.entries(webToolingFiles)) mockFiles[path] = content;
    setSetting("tools.fileWatchers", webToolingWatchers as never, "project");
  });

  it("plans watcher runs for root files of a changed partial", async () => {
    const runs = await fileWatchersApi.plan(["assets/scss/_variables.scss"], "save");
    expect(runs.map((r) => r.file)).toEqual(["assets/scss/app.scss", "assets/scss/print.scss"]);
    expect(runs[0].commandLine).toBe("sass app.scss:app.css");
    const templates = await fileWatchersApi.templates();
    expect(templates.map((t) => t.id)).toContain("babel");
    expect(watcherFromObject(webToolingWatchers[0]).trackOnlyRoot).toBe(true);
    const problems = await fileWatchersApi.problems("  app.scss 3:10  root stylesheet\nError: Undefined variable.", ["$FILE_PATH$ $LINE$:$COLUMN$"], "~/Projects/x/css");
    expect(problems[0]).toMatchObject({ line: 3, column: 10 });
  });

  it("lists OpenAPI endpoints and generates requests", async () => {
    const files = await openApiApi.endpoints();
    const api = files.find((f) => f.path === "apps/api/openapi.yaml")!;
    expect(api.endpoints.map((e) => `${e.method} ${e.path}`)).toContain("POST /orders");
    const req = await openApiApi.httpRequest(api.path, webToolingFiles[api.path], "GET", "/products/{id}");
    expect(req).toContain("GET {{baseUrl}}/products/{{id}}");
  });

  it("validates and completes GraphQL against the demo schema", async () => {
    const problems = await graphqlApi.validate("graphql/products.graphql", webToolingFiles["graphql/products.graphql"]);
    expect(problems.some((p) => p.message.includes("colour"))).toBe(true);
    const q = "query { product(id: 1) { ";
    const c = await graphqlApi.complete(q, q.length);
    expect(c.items.map((i) => i.label)).toContain("reviews");
  });

  it("reads the Nx workspace, Nuxt pages and aliases", async () => {
    const ws = await nxApi.workspace();
    expect(ws.projects.find((p) => p.name === "storefront")?.dependencies).toEqual(["@acme/ui"]);
    const nuxt = await nuxtApi.project("apps/storefront");
    expect(nuxt.routes.map((r) => r.route)).toContain("/products/:id");
    const text = 'import { formatPrice } from "@ui/format";';
    const target = await importsApi.goto("apps/storefront/pages/index.vue", text, text.indexOf("@ui") + 2);
    expect(target?.path).toBe("libs/ui/src/format.ts");
  });

  it("converts cURL commands", async () => {
    const http = await httpExtrasApi.fromCurl("curl -X DELETE https://api.acme.test/v2/products/1 -H 'Accept: application/json'");
    expect(http).toBe("DELETE https://api.acme.test/v2/products/1\nAccept: application/json\n");
  });
});
