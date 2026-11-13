import { describe, expect, it } from "vitest";
import { invoke } from "@/ipc";
import type { FileStructure, GenerateTargets, Hover, TextEdit } from "@/features/php/api";
import { demoFiles } from "./demoProject";
import { parsePhp } from "./phpMockParser";
import "./phpMock";

const apply = (src: string, edits: TextEdit[]) =>
  [...edits].sort((a, b) => b.start - a.start).reduce((s, e) => s.slice(0, e.start) + e.newText + s.slice(e.end), src);

describe("php mock parser", () => {
  it("parses demo classes and members", () => {
    const ctrl = parsePhp(demoFiles["app/Http/Controllers/OrderController.php"]).classes[0];
    expect(ctrl.name).toBe("OrderController");
    expect(ctrl.modifiers.isFinal).toBe(true);
    const show = ctrl.members.find((m) => m.name === "show")!;
    expect(show.kind).toBe("method");
    expect(show.attributes[0]).toContain("Route('/orders/{order}'");

    const user = parsePhp(demoFiles["app/Models/User.php"]).classes[0];
    expect(user.members.map((m) => m.name)).toEqual(["fillable", "hidden", "orders", "totalSpent", "casts"]);
    expect(user.members[0].doc).toContain("@var list<string>");
  });
});

describe("php mock backend", () => {
  const checkout = demoFiles["app/Services/Checkout.php"];
  const caret = checkout.indexOf("$taxRate;");

  it("lists generate targets for Checkout", async () => {
    const t = await invoke<GenerateTargets>("php_generate_targets", { path: "app/Services/Checkout.php", offset: caret, content: checkout });
    expect(t.className).toBe("Checkout");
    expect(t.hasConstructor).toBe(true);
    expect(t.fields.map((f) => [f.name, f.typeHint, f.inConstructor])).toEqual([
      ["currency", "string", true],
      ["taxRate", "float", true],
    ]);
  });

  it("generates getters and fluent setters", async () => {
    const edits = await invoke<TextEdit[]>("php_generate", {
      path: "app/Services/Checkout.php",
      offset: caret,
      kind: "gettersAndSetters",
      options: { fields: ["currency"], fluent: true },
      content: checkout,
    });
    const out = apply(checkout, edits);
    expect(out).toContain("    }\n\n    public function getCurrency(): string\n    {\n        return $this->currency;\n    }\n");
    expect(out).toContain("    public function setCurrency(string $currency): static\n    {\n        $this->currency = $currency;\n        return $this;\n    }\n}\n");
  });

  it("refuses a second constructor", async () => {
    await expect(
      invoke("php_generate", { path: "app/Services/Checkout.php", offset: caret, kind: "constructor", options: { fields: [] }, content: checkout }),
    ).rejects.toThrow("already has a constructor");
  });

  it("builds structure with inherited members", async () => {
    const s = await invoke<FileStructure>("php_file_structure", { path: "app/Models/Order.php", inherited: true });
    const kids = s.nodes[0].children!;
    expect(kids.find((k) => k.name === "user")?.detail).toBe("(): BelongsTo");
    expect(kids.some((k) => k.inheritedFrom === "Model" && k.name === "save")).toBe(true);
  });

  it("hovers methods with attributes", async () => {
    const src = demoFiles["app/Http/Controllers/OrderController.php"];
    const h = await invoke<Hover>("php_hover", { path: "app/Http/Controllers/OrderController.php", offset: src.indexOf("show(") + 1 });
    expect(h.markdown).toContain("#[Route('/orders/{order}', name: 'orders.show', methods: ['GET'])]");
    expect(h.markdown).toContain("public function show(Request $request, Order $order): JsonResponse");
  });
});
