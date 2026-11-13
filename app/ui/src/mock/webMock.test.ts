import { beforeAll, describe, expect, it } from "vitest";
import { npmApi, webApi } from "@/ipc/web";
import { forgeWasm } from "@/wasm";
import "./settingsMock";
import { mockFiles } from "./workspaceMock";
import "./webMock";
import { webDemos } from "./demoWebProjects";

describe("WebForge mock (Rust packs/webforge through WebAssembly)", () => {
  beforeAll(async () => {
    await forgeWasm();
    // Put the React and Angular demos next to the Laravel demo the tests load.
    for (const [path, content] of Object.entries(webDemos.react.files)) mockFiles[`storefront/${path}`] = content;
    for (const [path, content] of Object.entries(webDemos.angular.files)) mockFiles[`portal/${path}`] = content;
  });

  it("lists package.json scripts with their package manager", async () => {
    const pkgs = await npmApi.packages();
    const store = pkgs.find((p) => p.path === "storefront/package.json")!;
    expect(store.scripts.map((s) => s.name)).toContain("dev");
    expect(store.packageManager.manager).toBe("pnpm");
    expect(store.packageManager.reason).toEqual({ kind: "field", path: "storefront/package.json", value: "pnpm@9.15.4" });
    expect(pkgs.find((p) => p.path === "portal/package.json")?.packageManager.manager).toBe("npm");
    expect(pkgs[0].path).toBe("package.json");
  });

  it("marks scripts in package.json", async () => {
    const marks = await npmApi.gutter("storefront/package.json", webDemos.react.files["package.json"]);
    expect(marks[0]).toMatchObject({ kind: "run", title: "Run 'dev'", line: 7 });
  });

  it("navigates from JSX tags to React components", async () => {
    const app = mockFiles["storefront/src/App.tsx"];
    const targets = await webApi.goto("react", "storefront/src/App.tsx", app, app.indexOf("<CartDrawer") + 2);
    expect(targets).toEqual([{ path: "storefront/src/components/CartDrawer.tsx", line: 9, column: 16 }]);
  });

  it("links Angular components with their templates", async () => {
    const path = "portal/src/app/users/user-card.component.ts";
    const marks = await webApi.gutter("angular", path, mockFiles[path]);
    expect(marks.map((m) => m.kind)).toEqual(["template", "styles"]);
    const components = await webApi.components("angular");
    expect(components.find((c) => c.name === "UserCardComponent")?.usages).toBe(1);
  });
});
