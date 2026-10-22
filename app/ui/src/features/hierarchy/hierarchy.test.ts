import { describe, expect, it } from "vitest";
import { invoke } from "@/ipc";
import { commands } from "@/state/commands";
import { findToolWindow } from "@/features/registry";
import type { CallHierarchyAnswer, CallNode, TypeHierarchyAnswer } from "@/ipc/hierarchy";
import { demoLaravelFiles } from "@/mock/demoLaravel";
import { positionOf } from "@/mock/codeintelMock";
import "@/mock/hierarchyMock";
import "@/mock/workspaceMock";
import { hierarchyLoaded } from "./model";
import { rowsOf, type HierarchyRow } from "./rows";
import { ROOT_KEY, type CallTab, type MethodTab, type TypeTab } from "./state";
import "./index";

const gateway = demoLaravelFiles["app/Contracts/PaymentGateway.php"];
const stripe = demoLaravelFiles["app/Services/Payments/StripeGateway.php"];
const service = demoLaravelFiles["app/Services/Payments/PaymentService.php"];

function at(path: string, text: string, needle: string) {
  const i = text.indexOf(needle);
  if (i < 0) throw new Error(`no ${needle}`);
  const pos = positionOf(text, i + 1);
  return { path, line: pos.line, character: pos.character };
}

const base = { id: 1, title: "", origin: { path: "", pos: { line: 0, character: 0 } }, scope: "all" as const, sortAlphabetically: true, collapsed: [], expanded: [], selected: null };
const outline = (rows: HierarchyRow[]) => rows.map((r) => `${"  ".repeat(r.depth)}${r.item.name}${r.current ? "*" : ""}${r.mark ? ` ${r.mark}` : ""}`);

describe("hierarchy", () => {
  it("implements PHPStorm's hierarchy actions and the tool window on the right", () => {
    for (const id of ["TypeHierarchy", "MethodHierarchy", "CallHierarchy"]) expect(commands().has(id), id).toBe(true);
    expect(findToolWindow("hierarchy")?.anchor).toBe("right");
  });

  it("shows the type hierarchy views computed in Rust", async () => {
    await hierarchyLoaded;
    const answer = await invoke<TypeHierarchyAnswer>("code_type_hierarchy", at("app/Contracts/PaymentGateway.php", gateway, "PaymentGateway\n"));
    expect(answer.title).toBe("PaymentGateway");
    const tab: TypeTab = { ...base, kind: "type", view: "subtypes", graph: answer.graph! };
    expect(outline(rowsOf(tab))).toEqual(["PaymentGateway*", "  FakeGateway", "  StripeGateway"]);
    expect(outline(rowsOf({ ...tab, view: "supertypes" }))).toEqual(["PaymentGateway*"]);
    // Tests only: nothing implements the gateway in tests/, the current type stays.
    expect(outline(rowsOf({ ...tab, scope: "test" }))).toEqual(["PaymentGateway*"]);
    // Collapsed rows hide their children.
    expect(outline(rowsOf({ ...tab, collapsed: ["App\\Contracts\\PaymentGateway"] }))).toEqual(["PaymentGateway*"]);
  });

  it("marks where a method is defined", async () => {
    await hierarchyLoaded;
    const answer = await invoke<TypeHierarchyAnswer>("code_method_hierarchy", at("app/Services/Payments/StripeGateway.php", stripe, "charge("));
    expect(answer.title).toBe("StripeGateway::charge()");
    const tab: MethodTab = { ...base, kind: "method", graph: answer.graph!, hideNotImplemented: false };
    expect(outline(rowsOf(tab))).toEqual(["PaymentGateway abstract", "  FakeGateway defined", "  StripeGateway* defined"]);
  });

  it("loads callers lazily and keeps their call sites", async () => {
    await hierarchyLoaded;
    const answer = await invoke<CallHierarchyAnswer>("code_call_hierarchy", at("app/Contracts/PaymentGateway.php", gateway, "charge("));
    expect(answer.root?.name).toBe("charge()");
    const callers = await invoke<CallNode[]>("code_call_hierarchy_calls", { item: answer.root, direction: "callers" });
    const tab: CallTab = { ...base, kind: "call", root: answer.root!, direction: "callers", calls: {}, expanded: [ROOT_KEY] };
    expect(rowsOf(tab).map((r) => r.status ?? r.item.name)).toEqual(["charge()", "loading"]);
    const rows = rowsOf({ ...tab, calls: { [ROOT_KEY]: callers } });
    const pay = rows.find((r) => r.item.name === "pay()")!;
    expect(pay.item.path).toBe("app/Services/Payments/PaymentService.php");
    expect(pay.calls?.[0].preview).toContain("$this->gateway->charge($order)");
    expect(pay.hasChildren).toBe(true);
    expect(service).toContain("public function pay");
    // Callees of `pay()`: the gateway call and the exception.
    const callees = await invoke<CallNode[]>("code_call_hierarchy_calls", { item: pay.item, direction: "callees" });
    expect(callees.map((c) => c.item.name)).toEqual(["charge()", "__construct()"]);
  });
});
