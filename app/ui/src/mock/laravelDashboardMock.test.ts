import { describe, expect, it } from "vitest";
import { generate, logPage, overview, parseLog } from "./laravelDashboardMock";
import { mockFiles } from "./workspaceMock";

describe("Laravel tool window mock", () => {
  it("parses the demo log with exceptions and project frames", () => {
    const page = logPage();
    expect(page.path).toBe("storage/logs/laravel.log");
    const error = page.entries.find((e) => e.level === "error")!;
    expect(error.message).toBe("Payment declined by the gateway");
    expect(error.exception).toMatchObject({ class: "App\\Exceptions\\PaymentFailed", code: "402", frame: { path: "app/Services/Payments/StripeGateway.php", line: 31 } });
    expect(error.frames?.[1]).toMatchObject({ path: "app/Http/Controllers/OrderController.php", line: 243 });
    expect(error.frames?.at(-1)?.call).toBe("{main}");
    expect(page.entries[0]).toMatchObject({ level: "info", message: "Order placed", context: '{"order":1042,"user":7} []' });
    expect(parseLog("[2026-10-26T10:00:00.5+00:00] prod.ALERT: Disk full\n")[0]).toMatchObject({ time: "2026-10-26 10:00:00", level: "alert", channel: "prod" });
  });

  it("summarises the application and runs ide.json code generations", () => {
    const o = overview();
    expect(o.appName).toBeTruthy();
    expect(o.commands.map((c) => c.name)).toContain("carts:prune");
    expect(o.ideJson.codeGenerations[0].name).toBe("Create Data Object");
    const files = generate("acme:dto", "Orders/invoice", true);
    expect(files[0].path).toBe("app/Data/Orders/InvoiceData.php");
    expect(mockFiles["app/Data/Orders/InvoiceData.php"]).toContain("final readonly class InvoiceData");
    expect(generate("acme:dto", "Orders/invoice", false)[0].exists).toBe(true);
  });
});
