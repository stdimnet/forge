import { describe, expect, it } from "vitest";
import { symfonyDemoFiles } from "./demoSymfonyProject";
import { mockFiles } from "./workspaceMock";
import { createTranslation, eventGutter, extrasComplete, parseYamlCatalogue, translationAnalysis, translationTable } from "./symfonyExtrasMock";

// The tests run with the Laravel demo; add the Symfony catalogues and listeners.
for (const [p, text] of Object.entries(symfonyDemoFiles)) if (/^(translations|src\/Event)/.test(p)) mockFiles[p] = text;

const at = (src: string) => [src.replace("¦", ""), src.indexOf("¦")] as const;

describe("symfony translations mock", () => {
  it("flattens YAML catalogues and builds the table", () => {
    expect(parseYamlCatalogue("order:\n    status:\n        paid: Paid\n    pay: 'Pay now'\n")).toEqual([
      { key: "order.status.paid", value: "Paid", line: 2 },
      { key: "order.pay", value: "Pay now", line: 3 },
    ]);
    const t = translationTable();
    expect(t.locales).toEqual(["en", "de", "fr"]);
    expect(t.domains).toEqual(["admin", "messages"]);
    expect(t.rows.find((r) => r.key === "order.back")?.missing).toEqual(["de"]);
  });

  it("reports missing keys, folds known ones and creates keys", () => {
    const twig = symfonyDemoFiles["templates/order/show.html.twig"];
    const a = translationAnalysis("templates/order/show.html.twig", twig);
    expect(a.folds.map((f) => f.text)).toContain("Order #%id%");
    const missing = a.problems.find((p) => p.key === "order.shipping_note")!;
    expect(missing.severity).toBe("warning");
    expect(missing.locales).toEqual(["de", "en", "fr"]);
    expect(a.problems.find((p) => p.key === "order.back")).toMatchObject({ severity: "weak", locales: ["de"] });
    createTranslation("messages", "order.shipping_note", missing.locales);
    expect(translationAnalysis("templates/order/show.html.twig", twig).problems.some((p) => p.key === "order.shipping_note")).toBe(false);
  });

  it("completes keys, events and form options", () => {
    let [src, pos] = at("{{ 'order.st¦'|trans }}");
    expect(extrasComplete("t.html.twig", src, pos)?.items.map((i) => i.label)).toEqual(["order.status.label", "order.status.new", "order.status.paid"]);
    [src, pos] = at("#[AsEventListener(event: 'kernel.re¦')]");
    expect(extrasComplete("src/X.php", src, pos)?.items.map((i) => i.label)).toEqual(["kernel.request", "kernel.response"]);
    [src, pos] = at("->add('price', MoneyType::class, ['currency' => 'EUR', 'div¦");
    expect(extrasComplete("src/Form/ProductType.php", src, pos)?.items[0]).toMatchObject({ label: "divisor", detail: "MoneyType" });
  });

  it("marks listeners and events in the gutter", () => {
    const path = "src/Event/OrderPlacedEvent.php";
    expect(eventGutter(path, symfonyDemoFiles[path])[0]).toMatchObject({ kind: "event", targets: [{ path: "src/EventListener/OrderMailListener.php" }, { path: "src/EventSubscriber/LocaleSubscriber.php" }] });
    const sub = "src/EventSubscriber/LocaleSubscriber.php";
    expect(eventGutter(sub, symfonyDemoFiles[sub]).map((m) => m.title)).toContain("Listens to kernel.request (priority 20)");
  });
});
