import { describe, expect, it } from "vitest";
import { invoke } from "@/ipc";
import type {
  DeclarationAnswer,
  HighlightAnswer,
  LibraryFile,
  LineMarkerInfo,
  NavigateAnswer,
  SymbolHover,
  UsagesAnswer,
  UsageTarget,
} from "@/ipc/codeintel";
import { demoLaravelFiles } from "./demoLaravel";
import { demoFiles } from "./demoProject";
import { offsetOf, positionOf } from "./codeintelMock";
import "./workspaceMock";

const stripe = demoLaravelFiles["app/Services/Payments/StripeGateway.php"];
const gateway = demoLaravelFiles["app/Contracts/PaymentGateway.php"];
const order = demoFiles["app/Models/Order.php"];

/** Arguments for a command at the `nth` occurrence of `needle` (caret one char inside). */
function at(path: string, text: string, needle: string, nth = 0) {
  let i = -1;
  for (let k = 0; k <= nth; k++) i = text.indexOf(needle, i + 1);
  if (i < 0) throw new Error(`no ${needle}`);
  const pos = positionOf(text, i + 1);
  return { path, line: pos.line, character: pos.character };
}

describe("code intelligence mock", () => {
  it("converts offsets and positions", () => {
    const text = "a\nbc\n";
    expect(positionOf(text, 3)).toEqual({ line: 1, character: 1 });
    expect(offsetOf(text, { line: 1, character: 1 })).toBe(3);
  });

  it("goes to PHP's own \\Throwable in the read-only stubs", async () => {
    const answer = await invoke<DeclarationAnswer>("code_goto_declaration", at("app/Services/Payments/StripeGateway.php", stripe, "Throwable"));
    expect(answer.onDeclaration).toBe(false);
    expect(answer.source).toBe("stubs");
    expect(answer.targets[0]).toMatchObject({ path: "library://php-stubs/Core/Core_c.php", name: "Throwable", container: "Core" });
    const file = await invoke<LibraryFile>("library_read", { path: answer.targets[0].path });
    expect(file.library).toBe("PHP Runtime");
    expect(file.text.split("\n")[answer.targets[0].line]).toContain("interface Throwable");
  });

  it("resolves members through imports and property types", async () => {
    const service = demoLaravelFiles["app/Services/Payments/PaymentService.php"];
    const answer = await invoke<DeclarationAnswer>("code_goto_declaration", at("app/Services/Payments/PaymentService.php", service, "charge"));
    expect(answer.targets[0]).toMatchObject({ path: "app/Contracts/PaymentGateway.php", name: "PaymentGateway::charge()" });
    // A method of a built-in class through a catch variable.
    const msg = await invoke<DeclarationAnswer>("code_goto_declaration", at("app/Services/Payments/StripeGateway.php", stripe, "getMessage"));
    expect(msg.targets[0]).toMatchObject({ name: "Throwable::getMessage()", path: "library://php-stubs/Core/Core_c.php" });
  });

  it("answers Ctrl+hover with the reference range and its declaration", async () => {
    const info = await invoke<SymbolHover>("code_symbol_info", at("app/Services/Payments/StripeGateway.php", stripe, "Throwable"));
    expect(info.signature).toBe("interface Throwable extends Stringable");
    expect(info.container).toBe("PHP Runtime · Core");
    expect(info.range.end.character - info.range.start.character).toBe("\\Throwable".length);
    expect(await invoke("code_symbol_info", at("app/Services/Payments/StripeGateway.php", stripe, "function"))).toBeNull();
  });

  it("shows usages on a declaration, grouped by usage type", async () => {
    const decl = await invoke<DeclarationAnswer>("code_goto_declaration", at("app/Models/Order.php", order, "Order extends"));
    expect(decl.onDeclaration).toBe(true);
    const usages = await invoke<UsagesAnswer>("code_find_usages", at("app/Models/Order.php", order, "Order extends"));
    expect(usages.title).toBe("Order");
    const labels = usages.byType.groups.map((g) => g.label);
    expect(labels).toContain("Usage in import");
    expect(labels).toContain("New instance creation");
    expect(labels).toContain("Static member access");
    expect(labels).toContain("Usage in type declaration");
    const imp = usages.byType.groups.find((g) => g.label === "Usage in import")!;
    const row = imp.files[0].usages[0];
    expect(row.text.slice(...row.ranges[0])).toBe("Order");
    expect(usages.byFile.fileCount).toBeGreaterThan(5);
  });

  it("finds call sites of an interface method, not the implementations", async () => {
    const usages = await invoke<UsagesAnswer>("code_find_usages", at("app/Contracts/PaymentGateway.php", gateway, "charge"));
    expect(usages.title).toBe("PaymentGateway::charge()");
    expect(usages.byType.groups.map((g) => g.label)).toEqual(["Method call"]);
    expect(usages.byFile.groups[0].files.map((f) => f.path)).toEqual([
      "app/Services/Payments/PaymentService.php",
      "tests/Unit/PaymentServiceTest.php",
    ]);
  });

  it("applies the Find Usages options and scopes", async () => {
    const args = at("app/Contracts/PaymentGateway.php", gateway, "PaymentGateway");
    const impl = await invoke<UsagesAnswer>("code_find_usages", {
      ...args,
      options: { interfaceUsages: false, interfaceImplementingClasses: true },
    });
    expect(impl.element).toBe("interface");
    expect(impl.byType.groups.map((g) => g.label)).toEqual(["Implementing classes"]);
    expect(impl.byFile.groups[0].files.map((f) => f.path).sort()).toEqual([
      "app/Services/Payments/FakeGateway.php",
      "app/Services/Payments/StripeGateway.php",
    ]);
    const inFile = await invoke<UsagesAnswer>("code_find_usages", { ...at("app/Services/Payments/StripeGateway.php", stripe, "Order $order"), scope: { kind: "currentFile" } });
    expect(inFile.scope).toBe("File 'StripeGateway.php'");
    expect(new Set(inFile.byFile.groups[0].files.map((f) => f.path))).toEqual(new Set(["app/Services/Payments/StripeGateway.php"]));
    const dir = await invoke<UsagesAnswer>("code_find_usages", { ...args, scope: { kind: "directory", directory: "tests" } });
    expect(dir.scope).toBe("Directory 'tests'");
    expect(dir.byFile.groups.flatMap((g) => g.files.map((f) => f.path)).every((p) => p.startsWith("tests/"))).toBe(true);
  });

  it("highlights usages in the file, exit points and implementing methods", async () => {
    const path = "app/Services/Payments/StripeGateway.php";
    const order = await invoke<HighlightAnswer>("code_highlight_usages", { ...at(path, stripe, "$order", 1), content: stripe });
    expect(order.target).toBe("usages");
    expect(order.highlights.map((h) => h.kind)).toEqual(["write", "read"]);
    expect(order.summary).toBe("2 usages of '$order' found");
    const exits = await invoke<HighlightAnswer>("code_highlight_usages", { ...at(path, stripe, "return"), content: stripe });
    expect(exits.target).toBe("exitPoints");
    expect(exits.highlights).toHaveLength(2);
    const methods = await invoke<HighlightAnswer>("code_highlight_usages", { ...at(path, stripe, "implements"), content: stripe });
    expect(methods.target).toBe("implementingMethods");
    expect(methods.summary).toBe("2 methods implementing 'PaymentGateway' found");
    const target = await invoke<UsageTarget>("code_usage_target", at("app/Contracts/PaymentGateway.php", gateway, "PaymentGateway"));
    expect(target.label).toBe("interface PaymentGateway");
    expect(target.fields.map((f) => f.label)).toContain("Implementing classes");
  });

  it("navigates to implementations and super methods", async () => {
    const impl = await invoke<NavigateAnswer>("code_navigate", { ...at("app/Contracts/PaymentGateway.php", gateway, "PaymentGateway"), kind: "implementation" });
    expect(impl.targets.map((t) => t.name).sort()).toEqual(["FakeGateway", "StripeGateway"]);
    const sup = await invoke<NavigateAnswer>("code_navigate", { ...at("app/Services/Payments/StripeGateway.php", stripe, "charge"), kind: "superMethod" });
    expect(sup.targets.map((t) => t.name)).toEqual(["PaymentGateway::charge()"]);
  });

  it("computes implemented / overridden gutter markers", async () => {
    const markers = await invoke<LineMarkerInfo[]>("code_line_markers", { path: "app/Contracts/PaymentGateway.php" });
    expect(markers.map((m) => [m.line, m.kind, m.title])).toEqual([
      [6, "implemented", "Is implemented in 2 classes"],
      [9, "implemented", "Is implemented in 2 classes"],
      [11, "implemented", "Is implemented in 2 classes"],
    ]);
    const up = await invoke<LineMarkerInfo[]>("code_line_markers", { path: "app/Services/Payments/StripeGateway.php" });
    expect(up.map((m) => [m.kind, m.title])).toEqual([
      ["implements", "Implements method in PaymentGateway"],
      ["implements", "Implements method in PaymentGateway"],
    ]);
    const controller = await invoke<LineMarkerInfo[]>("code_line_markers", { path: "app/Http/Controllers/Controller.php" });
    expect(controller[0].kind).toBe("subclassed");
    expect(controller[0].targets.length).toBeGreaterThanOrEqual(3);
  });
});
