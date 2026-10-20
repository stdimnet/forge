import { describe, expect, it } from "vitest";
import { mockSymfonyComplete, mockSymfonyGoto, mockSymfonyGutter, referenceAt } from "./symfonyMock";
import { doctrineSnapshot } from "./doctrineMock";
import type { DqlCompletion, DqlDiagnostic } from "@/ipc/doctrine";
import { callWasm, forgeWasm } from "@/wasm";
import { symfonyDemoFiles } from "./demoSymfonyProject";
import { splitCommandLine } from "@/features/symfony/console";

const at = (src: string) => [src.replace("¦", ""), src.indexOf("¦")] as const;

describe("symfony mock references", () => {
  it("detects route, template, service and parameter contexts", () => {
    let [src, pos] = at("$this->redirectToRoute('order_¦');");
    expect(referenceAt("src/X.php", src, pos)).toMatchObject({ kind: "route", prefix: "order_" });
    [src, pos] = at("{% extends 'ba¦' %}");
    expect(referenceAt("t.html.twig", src, pos)).toMatchObject({ kind: "template", prefix: "ba" });
    [src, pos] = at("        arguments: ['@app.inv¦']");
    expect(referenceAt("config/services.yaml", src, pos)).toMatchObject({ kind: "service", prefix: "app.inv" });
    [src, pos] = at("#[Autowire('%app.cur¦')]");
    expect(referenceAt("src/X.php", src, pos)).toMatchObject({ kind: "parameter", prefix: "app.cur" });
    [src, pos] = at("{{ total|pri¦ }}");
    expect(referenceAt("t.html.twig", src, pos)).toMatchObject({ kind: "filter", prefix: "pri" });
    [src, pos] = at("<p>text¦</p>");
    expect(referenceAt("t.html.twig", src, pos)).toBeNull();
  });

  it("completes and navigates like the backend", () => {
    const [src, pos] = at("<a href=\"{{ path('order_¦') }}\">");
    const list = mockSymfonyComplete("t.html.twig", src, pos)!;
    expect(list.items.map((i) => i.label)).toEqual(["order_index", "order_pay", "order_show"]);
    const [src2, pos2] = at("{{ path('order_sh¦ow') }}");
    expect(mockSymfonyGoto("t.html.twig", src2, pos2)[0]).toMatchObject({ path: "src/Controller/OrderController.php" });
  });

  it("computes gutter marks for controllers and templates", () => {
    const marks = mockSymfonyGutter("src/Controller/OrderController.php", symfonyDemoFiles["src/Controller/OrderController.php"]);
    expect(marks.filter((m) => m.kind === "route").map((m) => m.title)).toContain("GET /orders/{id} (order_show)");
    const tpl = mockSymfonyGutter("templates/order/show.html.twig", symfonyDemoFiles["templates/order/show.html.twig"]);
    expect(tpl[0].title).toBe("Rendered by OrderController::show");
  });
});

describe("doctrine mock DQL (Rust phpforge-dql via WebAssembly)", () => {
  const dql = async <T>(name: "dql_complete" | "dql_diagnostics", path: string, content: string, offset = 0) => {
    const wasm = await forgeWasm();
    return callWasm<T>(wasm[name], { path, content, offset, snapshot: doctrineSnapshot() });
  };

  it("completes fields of aliases declared through joins", async () => {
    const [src, pos] = at("<?php $em->createQuery('SELECT o FROM App\\Entity\\Order o JOIN o.customer c WHERE c.¦');");
    const c = await dql<DqlCompletion>("dql_complete", "src/X.php", src, pos);
    expect(c.items.map((i) => i.label)).toEqual(["id", "email", "name", "orders"]);
  });

  it("uses QueryBuilder aliases in repositories", async () => {
    const [src, pos] = at("<?php\nnamespace App\\Repository;\nclass OrderRepository {\n function f() { return $this->createQueryBuilder('o')->andWhere('o.st¦'); }\n}");
    const c = await dql<DqlCompletion>("dql_complete", "src/Repository/OrderRepository.php", src, pos);
    expect(c.items.map((i) => i.label)).toEqual(["status"]);
  });

  it("reports unknown fields", async () => {
    const src = "<?php $em->createQuery('SELECT o FROM App\\Entity\\Order o WHERE o.totl > 1 AND o.customer.email = 1');";
    const d = await dql<DqlDiagnostic[]>("dql_diagnostics", "src/X.php", src);
    expect(d.map((x) => src.slice(x.start, x.end))).toEqual(["totl"]);
  });
});

describe("console command line", () => {
  it("splits the command name from its arguments", () => {
    expect(splitCommandLine("cache:clear --env=prod  --no-warmup ")).toEqual({ command: "cache:clear", args: "--env=prod  --no-warmup" });
    expect(splitCommandLine("about")).toEqual({ command: "about", args: "" });
  });
});
