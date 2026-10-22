import { describe, expect, it } from "vitest";
import type { ClassMember, HierarchyInfo } from "@/ipc/refactor";
import type { ContextAction } from "@/ipc/codeintel";
import { commands } from "@/state/commands";
import {
  abstractBox,
  canChoose,
  dependencies,
  initialChoice,
  suggestedName,
  validateTypeName,
} from "./hierarchyRefactorings";
import { indexOfMnemonic, itemsOf, matches, mnemonic } from "./refactorThis";
import { phpRefactorings } from "@/mock/refactorHierarchyMock";
import "./index";

const member = (id: string, patch: Record<string, unknown> = {}): ClassMember =>
  ({
    id,
    name: id.replace(/[$()]/g, ""),
    kind: id.endsWith("()")
      ? "method"
      : id.startsWith("$")
        ? "property"
        : "constant",
    signature: id,
    visibility: "public",
    isStatic: false,
    isAbstract: false,
    constructor: id === "__construct()",
    uses: [],
    ...patch,
  }) as ClassMember;

const INFO: HierarchyInfo = {
  path: "app/Models/User.php",
  position: { line: 5, character: 6 },
  directory: "app/Models",
  name: "User",
  fqn: "App\\Models\\User",
  namespace: "App\\Models",
  kind: "class",
  isAbstract: false,
  parent: "App\\Models\\Model",
  interfaces: [],
  members: [
    member("ROLE"),
    member("$email", { visibility: "private" }),
    member("__construct()"),
    member("orders()", { uses: ["$email", "ROLE"] }),
    member("secret()", { visibility: "protected" }),
  ],
  selected: "orders()",
  targets: [],
};

describe("hierarchy refactoring dialogs", () => {
  it("offers the members each refactoring can take", () => {
    const ids = (mode: Parameters<typeof canChoose>[1]) =>
      INFO.members.filter((m) => canChoose(m, mode)).map((m) => m.id);
    expect(ids("interface")).toEqual(["ROLE", "orders()"]);
    expect(ids("pullUpInterface")).toEqual(["ROLE", "orders()"]);
    expect(ids("superclass")).toHaveLength(5);
  });

  it("has abstract boxes for methods only", () => {
    expect(abstractBox(INFO.members[3], "superclass", "class")).toEqual({
      enabled: true,
      implied: false,
    });
    expect(abstractBox(INFO.members[0], "superclass", "class")).toBeNull();
    expect(abstractBox(INFO.members[2], "pullUpClass", "class")).toBeNull();
    expect(abstractBox(INFO.members[3], "pullUpInterface", "class")).toEqual({
      enabled: false,
      implied: true,
    });
    expect(abstractBox(INFO.members[3], "pushDown", "interface")).toBeNull();
  });

  it("highlights what the chosen members use", () => {
    expect([...dependencies(INFO.members, new Set(["orders()"]))]).toEqual([
      "$email",
      "ROLE",
    ]);
    expect([
      ...dependencies(INFO.members, new Set(["orders()", "ROLE"])),
    ]).toEqual(["$email"]);
    expect([...initialChoice(INFO, "interface")]).toEqual(["orders()"]);
    expect([
      ...initialChoice({ ...INFO, selected: "secret()" }, "interface"),
    ]).toEqual([]);
  });

  it("suggests and checks the new type's name", () => {
    expect(suggestedName(INFO, "interface")).toBe("UserInterface");
    expect(suggestedName(INFO, "superclass")).toBe("AbstractUser");
    expect(validateTypeName("HasOrders", INFO)).toBeNull();
    expect(validateTypeName("1x", INFO)).toBe("'1x' is not a valid class name");
    expect(validateTypeName("user", INFO)).toBe(
      "The name should differ from User",
    );
  });

  it("implements PHPStorm's actions", () => {
    for (const id of [
      "ExtractInterface",
      "ExtractSuperclass",
      "MembersPullUp",
      "MemberPushDown",
    ])
      expect(commands().has(id), id).toBe(true);
  });
});

describe("Refactor This popup", () => {
  it("numbers rows 1-9 and 0", () => {
    expect([0, 8, 9, 10].map(mnemonic)).toEqual(["1", "9", "0", ""]);
    expect(indexOfMnemonic("0", 12)).toBe(9);
    expect(indexOfMnemonic("3", 2)).toBeNull();
    expect(indexOfMnemonic("a", 5)).toBeNull();
  });

  it("filters by typed characters", () => {
    expect(matches("Extract Interface…", "exi")).toBe(true);
    expect(matches("Extract Interface…", "ext int")).toBe(true);
    expect(matches("Rename…", "rx")).toBe(false);
  });

  it("lists enabled refactorings, then the language server's", () => {
    const lsp = [
      {
        title: "Convert to arrow function",
        kind: "refactor",
        source: "lsp",
        isPreferred: false,
        needsResolve: false,
      },
      {
        title: "Move to a new file",
        kind: "refactor",
        source: "lsp",
        isPreferred: false,
        needsResolve: false,
        disabled: "Selection is not valid",
      },
    ] as ContextAction[];
    const items = itemsOf(
      ["RenameElement", "MakeStatic"],
      lsp,
      {
        available: (id) => id !== "MakeStatic",
        title: (id) => (id === "RenameElement" ? "Rename…" : id),
        shortcut: () => "Shift+F6",
        run: () => undefined,
      },
      () => undefined,
    );
    expect(items.map((i) => [i.title, !!i.separator])).toEqual([
      ["Rename…", false],
      ["Convert to arrow function", true],
    ]);
  });

  it("offers what applies at the caret in the demo files", () => {
    const src =
      "<?php\nnamespace App;\n\nclass Cart extends Base\n{\n    public function total(): int\n    {\n        return $this->sum * 2;\n    }\n}\n";
    expect(
      phpRefactorings(src, src.indexOf("Cart") + 1, src.indexOf("Cart") + 1),
    ).toContain("ExtractInterface");
    expect(
      phpRefactorings(src, src.indexOf("total") + 1, src.indexOf("total") + 1),
    ).toContain("ChangeSignature");
    const at = src.indexOf("$this");
    expect(phpRefactorings(src, at, at + 15)).toContain("ExtractMethod");
  });
});
