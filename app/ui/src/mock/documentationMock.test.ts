import { describe, expect, it } from "vitest";
import { invoke } from "@/ipc";
import type { ContextLine, ExternalDocAnswer, QuickDefinitionAnswer } from "@/ipc/codeintel";
import { demoLaravelFiles } from "./demoLaravel";
import { demoFiles } from "./demoProject";
import { positionOf } from "./codeintelMock";
import "./lspMock";
import "./phpMock";
import "./documentationMock";
import "./workspaceMock";

const user = demoFiles["app/Models/User.php"];
const stripe = demoLaravelFiles["app/Services/Payments/StripeGateway.php"];

/** Arguments at the `nth` occurrence of `needle` (caret one char inside). */
function at(path: string, text: string, needle: string, nth = 0) {
  let i = -1;
  for (let k = 0; k <= nth; k++) i = text.indexOf(needle, i + 1);
  if (i < 0) throw new Error(`no ${needle}`);
  return { path, ...positionOf(text, i + 1) };
}

describe("documentation mock", () => {
  it("shows the declaration's source with its docblock for Quick Definition", async () => {
    const answer = await invoke<QuickDefinitionAnswer>("code_quick_definition", at("app/Models/User.php", user, "Order::class"));
    expect(answer.definitions).toHaveLength(1);
    const def = answer.definitions[0];
    expect(def.path).toBe("app/Models/Order.php");
    expect(def.text).toMatch(/class Order/);
    expect(def.text.trimEnd().endsWith("}")).toBe(true);
  });

  it("shows the declaration itself on a declaration", async () => {
    const answer = await invoke<QuickDefinitionAnswer>("code_quick_definition", at("app/Models/User.php", user, "totalSpent"));
    expect(answer.definitions[0].text).toMatch(/^ {4}public function totalSpent\(\): float/);
    expect(answer.definitions[0].text.split("\n")).toHaveLength(6);
  });

  it("opens php.net for PHP's own symbols and nothing for project classes", async () => {
    const throwable = await invoke<ExternalDocAnswer>("code_external_doc", at("app/Services/Payments/StripeGateway.php", stripe, "Throwable"));
    expect(throwable.url).toBe("https://www.php.net/manual/en/class.throwable.php");
    const order = await invoke<ExternalDocAnswer>("code_external_doc", at("app/Models/User.php", user, "Order::class"));
    expect(order).toEqual({ url: null, name: "Order" });
  });

  it("reads types from the hover and the index", async () => {
    expect(await invoke<string | null>("code_type_info", at("app/Models/User.php", user, "totalSpent"))).toBe("float");
    expect(await invoke<string | null>("code_type_info", at("app/Models/User.php", user, "hasMany"))).toBe("HasMany");
  });

  it("lists the enclosing class and method for Context Info", async () => {
    const line = positionOf(user, user.indexOf("->where(")).line;
    const headers = await invoke<ContextLine[]>("code_context_info", { path: "app/Models/User.php", line });
    expect(headers.map((h) => h.text)).toEqual(["class User extends Authenticatable", "    public function totalSpent(): float"]);
  });
});
