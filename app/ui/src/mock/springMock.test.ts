import { describe, expect, it } from "vitest";
import type { ConfigProblem, SpringCompletion, SpringMarker, SpringOverview } from "@/ipc/spring";
import type { ServiceDetails, ServiceNode } from "@/ipc/services";

// The Spring Boot demo is chosen by the page URL, before the mocks load.
history.replaceState(null, "", "/?project=spring");
const { invoke } = await import("@/ipc");
await import("./settingsMock");
const { mockFiles } = await import("./workspaceMock");
await import("./springMock");

const PKG = "src/main/java/com/acme/inventory";
const YAML = "src/main/resources/application.yml";

describe("Spring Boot mock (javaforge-fw-spring through WebAssembly)", () => {
  it("reads beans, endpoints, entities and configuration of the demo", async () => {
    const o = await invoke<SpringOverview>("spring_model", { refresh: true });
    expect(o.spring && o.jakarta).toBe(true);
    const endpoints = o.endpoints.map((e) => `${e.method} ${e.path} ${e.framework}`);
    expect(endpoints).toContain("GET /api/stock/{sku} spring");
    expect(endpoints).toContain("DELETE /api/stock/{sku}/items/{quantity} spring");
    expect(endpoints).toContain("GET /rest/warehouses/{city}/stock jaxrs");
    const service = o.beans.find((b) => b.name === "stockService");
    expect(service?.kind).toBe("service");
    expect(service?.usages).toBe(3);
    expect(o.beans.find((b) => b.name === "productRepository")?.kind).toBe("dataRepository");
    expect(o.beans.some((b) => b.name === "inventory-com.acme.inventory.config.InventoryProperties")).toBe(true);
    const product = o.entities.find((e) => e.name === "Product");
    expect(product?.table).toBe("products");
    expect(product?.repositories).toEqual(["com.acme.inventory.stock.ProductRepository"]);
    expect(o.profiles).toEqual(expect.arrayContaining(["dev", "prod", "default"]));
    expect(o.metadata.properties).toBeGreaterThan(30);
    expect(o.applications.find((a) => a.kind === "spring")?.mainClass).toBe("com.acme.inventory.InventoryApplication");
  });

  it("completes and inspects application.yml", async () => {
    const content = "spring:\n  jpa:\n    sh";
    const c = await invoke<SpringCompletion>("spring_config_complete", { path: YAML, content, offset: content.length });
    const showSql = c.items.find((i) => i.label === "show-sql");
    expect(showSql?.insert).toBe("show-sql: $0");
    expect(showSql?.documentation).toContain("Whether to enable logging of SQL statements.");
    const value = "spring:\n  main:\n    banner-mode: ";
    const v = await invoke<SpringCompletion>("spring_config_complete", { path: YAML, content: value, offset: value.length });
    expect(v.items.map((i) => i.label)).toEqual(["off", "console", "log"]);
    const problems = await invoke<ConfigProblem[]>("spring_config_inspect", { path: YAML, content: mockFiles[YAML] });
    expect(problems.map((p) => [p.code, mockFiles[YAML].slice(p.from, p.to)])).toEqual([["unknownProperty", "reorder-mail"]]);
  });

  it("links beans and injection points in the gutter", async () => {
    const path = `${PKG}/stock/StockService.java`;
    const marks = await invoke<SpringMarker[]>("spring_gutter", { path, content: mockFiles[path] });
    expect(marks[0]).toMatchObject({ kind: "bean", line: 11 });
    const listeners = marks.filter((m) => m.kind === "injection").find((m) => m.targets.length === 2);
    expect(listeners?.targets.map((t) => t.preview).sort()).toEqual(["auditLog (AuditLog)", "reorderMailer (ReorderMailer)"]);
  });

  it("writes HTTP Client requests", async () => {
    const o = await invoke<SpringOverview>("spring_model");
    const find = o.endpoints.find((e) => e.path === "/api/stock/{sku}")!;
    const text = await invoke<string>("spring_http_request", { endpoint: find });
    expect(text).toBe("@sku = value\n\n### StockController.find\nGET http://localhost:8080/api/stock/{{sku}}\nAccept: application/json\n");
  });

  it("lists Spring Boot configurations in the run dashboard", async () => {
    // The demo's configurations are saved after the mocks loaded.
    await new Promise((r) => setTimeout(r, 0));
    const nodes = await invoke<ServiceNode[]>("spring_dashboard");
    expect(nodes[0].id).toBe("spring");
    const apps = nodes[0].children?.flatMap((g) => g.children ?? []).map((n) => n.label);
    expect(apps).toEqual(["InventoryApplication", "InventoryApplication (prod)"]);
    const details = await invoke<ServiceDetails>("spring_dashboard_details", { nodeId: "spring/app/InventoryApplication" });
    expect(details.actions).toEqual(["start"]);
    expect(details.sections[0].rows?.find((r) => r.label === "Active profiles")?.value).toBe("dev");
  });
});
