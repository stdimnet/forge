import { describe, expect, it } from "vitest";
import { invoke, listen } from "@/ipc";
import type { ArtisanEvent, GutterMark, LaravelCompletions, LaravelLocation, RoutesResult } from "@/ipc/laravel";
import { artisanEvent } from "@/ipc/laravel";
import { mockConfig, mockModels, mockTranslations } from "./laravelMock";
import { mockFiles } from "./workspaceMock";
import "./laravelMock";

const caret = (marked: string) => ({ content: marked.replace("|", ""), offset: marked.indexOf("|") });

async function labels(path: string, marked: string): Promise<string[]> {
  const r = await invoke<LaravelCompletions | null>("laravel_complete", { path, ...caret(marked) });
  return r?.items.map((i) => i.label) ?? [];
}

describe("Laravel mock", () => {
  it("parses demo routes with groups and resources", async () => {
    const { routes } = await invoke<RoutesResult>("laravel_routes");
    const byName = (n: string) => routes.find((r) => r.name === n)!;
    expect(byName("orders.show").action).toBe("App\\Http\\Controllers\\OrderController@show");
    expect(byName("orders.show").actionLocation?.path).toBe("app/Http/Controllers/OrderController.php");
    expect(byName("checkout.store").middleware).toEqual(["web", "auth", "verified"]);
    expect(byName("admin.dashboard").uri).toBe("admin");
    expect(byName("products.show").uri).toBe("products/{product}");
    expect(routes.some((r) => r.uri === "api/products/{product}")).toBe(true);
    expect(byName("about").action).toBe("view: pages.about");
  });

  it("reads config, translations and models", () => {
    expect(mockConfig().find((c) => c.key === "shop.pagination.products")?.value).toBe("24");
    expect(mockTranslations().map((t) => t.key)).toContain("messages.cart.empty");
    const product = mockModels().find((m) => m.name === "Product")!;
    expect(product.attributes.map((a) => a.name)).toEqual(["id", "name", "slug", "price", "stock", "published_at", "created_at", "updated_at"]);
    expect(product.scopes.map((s) => s.name)).toEqual(["available"]);
  });

  it("completes Laravel strings, properties and components", async () => {
    expect(await labels("x.php", "<?php return redirect()->route('|")).toContain("checkout.create");
    expect(await labels("x.php", "<?php view('|')")).toContain("orders.show");
    expect(await labels("x.php", "<?php config('shop.|")).toContain("shop.currency");
    expect(await labels("x.php", "<?php env('|")).toContain("APP_NAME");
    expect(await labels("x.blade.php", "{{ __('|') }}")).toContain("messages.welcome");
    expect(await labels("x.php", "<?php use App\\Models\\Product; Product::query()->where('|")).toContain("price");
    expect(await labels("x.php", "<?php use App\\Models\\User; User::with('|")).toEqual(["orders"]);
    expect(await labels("x.blade.php", "{{ $product->|")).toContain("slug");
    expect(await labels("x.blade.php", "<x-|")).toEqual(["x-alert", "x-button", "x-layout"]);
    expect(await labels("x.php", "<?php strlen('|")).toEqual([]);
  });

  it("goes to declarations", async () => {
    const goto = (path: string, marked: string) => invoke<LaravelLocation[]>("laravel_goto", { path, ...caret(marked) });
    const route = await goto("x.php", "<?php route('orders.sh|ow')");
    expect(route.map((l) => [l.path, l.line])).toEqual([
      ["routes/web.php", 11],
      ["app/Http/Controllers/OrderController.php", 17],
    ]);
    expect((await goto("x.php", "<?php view('products.in|dex')"))[0].path).toBe("resources/views/products/index.blade.php");
    expect((await goto("x.blade.php", "<x-al|ert />"))[0].path).toBe("resources/views/components/alert.blade.php");
  });

  it("marks routed controller methods and view calls", async () => {
    const path = "app/Http/Controllers/ProductController.php";
    const marks = await invoke<GutterMark[]>("laravel_gutter", { path, content: mockFiles[path] });
    expect(marks.map((m) => [m.line, m.kind])).toEqual([
      [9, "route"],
      [13, "view"],
      [16, "route"],
      [18, "view"],
    ]);
    expect(marks[0].tooltip).toBe("GET|HEAD /products (products.index)\nGET|HEAD /api/products (products.index)");
  });

  it("streams simulated artisan output", async () => {
    const events: ArtisanEvent[] = [];
    const off = await listen<ArtisanEvent>(artisanEvent("t1"), (e) => events.push(e));
    const code = await invoke<number>("laravel_artisan_run", { commandLine: "route:list", runId: "t1" });
    off();
    expect(code).toBe(0);
    expect(events[0]).toEqual({ type: "started", commandLine: "php artisan route:list" });
    expect(events.some((e) => e.type === "output" && e.text.includes("orders.show"))).toBe(true);
    expect(events.at(-1)).toEqual({ type: "exit", code: 0 });
  });
});
