import { describe, expect, it } from "vitest";
import type { FieldTarget, GenerateTargets, StructureNode } from "./api";
import { chooserTitle, clampToViewport, fieldsFor, generateMenu, groupByOwner, showsFluentOption } from "./generateModel";
import { highlightPhp, renderMarkdown } from "./markdown";
import { orderNodes } from "./StructureView";
import { toLspSettings, defaultPhpSettings, legacyPhpChanges } from "./settings";
import { percent } from "./status";

const field = (name: string, extra: Partial<FieldTarget> = {}): FieldTarget => ({
  name,
  visibility: "private",
  isStatic: false,
  isReadonly: false,
  promoted: false,
  hasGetter: false,
  hasSetter: false,
  inConstructor: false,
  ...extra,
});

const targets = (extra: Partial<GenerateTargets> = {}): GenerateTargets => ({
  className: "Checkout",
  classKind: "class",
  fields: [field("currency"), field("rate", { hasGetter: true }), field("id", { isReadonly: true, promoted: true }), field("count", { isStatic: true })],
  hasConstructor: false,
  hasToString: false,
  overrideMethods: [],
  implementMethods: [],
  ...extra,
});

describe("generate menu", () => {
  it("lists PHPStorm's actions in order and disables unavailable ones", () => {
    const items = generateMenu(targets({ hasConstructor: true }));
    expect(items.map((i) => i.label)).toEqual([
      "Constructor",
      "Getters",
      "Setters",
      "Getters and Setters",
      "Override Methods…",
      "Implement Methods…",
      "toString()",
      "PHPDoc Comment",
    ]);
    const enabled = Object.fromEntries(items.map((i) => [i.kind, i.enabled]));
    expect(enabled).toMatchObject({ constructor: false, getters: true, overrideMethods: false, phpDoc: false });
  });

  it("disables everything outside a class except PHPDoc", () => {
    const items = generateMenu({ fields: [], hasConstructor: false, hasToString: false, overrideMethods: [], implementMethods: [], phpDocTarget: "helper" });
    expect(items.filter((i) => i.enabled).map((i) => i.kind)).toEqual(["phpDoc"]);
  });

  it("filters chooser fields per action", () => {
    const t = targets();
    expect(fieldsFor("constructor", t).map((f) => f.name)).toEqual(["currency", "rate"]);
    expect(fieldsFor("getters", t).map((f) => f.name)).toEqual(["currency", "id", "count"]);
    expect(fieldsFor("setters", t).map((f) => f.name)).toEqual(["currency", "rate", "count"]);
    expect(fieldsFor("toString", t).map((f) => f.name)).toEqual(["currency", "rate", "id"]);
    expect(chooserTitle("constructor")).toBe("Choose Fields to Initialize by Constructor");
    expect(showsFluentOption("setters")).toBe(true);
    expect(showsFluentOption("getters")).toBe(false);
  });

  it("groups methods by owner", () => {
    const m = (owner: string, name: string) => ({ id: `${owner}::${name}`, name, owner, ownerKind: "class" as const, signature: `${name}()`, visibility: "public" as const, isStatic: false });
    const groups = groupByOwner([m("A", "x"), m("B", "y"), m("A", "z")]);
    expect(groups.map((g) => [g.owner, g.methods.map((x) => x.name)])).toEqual([
      ["A", ["x", "z"]],
      ["B", ["y"]],
    ]);
  });

  it("keeps popups inside the viewport", () => {
    expect(clampToViewport({ x: 990, y: 790 }, { width: 200, height: 100 }, { width: 1000, height: 800 })).toEqual({ x: 792, y: 692 });
    expect(clampToViewport({ x: -5, y: 3 }, { width: 10, height: 10 }, { width: 1000, height: 800 })).toEqual({ x: 8, y: 8 });
  });
});

describe("markdown", () => {
  it("renders fences, inline code, bold and lists safely", () => {
    const html = renderMarkdown("`App\\Models`\n\n```php\n#[Route('/x')]\npublic function show(): string\n```\n\n**Returns:** `<b>`\n\n- one\n- *two*");
    expect(html).toContain("<p><code>App\\Models</code></p>");
    expect(html).toContain('<span class="md-attr">#[</span>Route(<span class="md-str">\'/x\'</span>)<span class="md-attr">]</span>');
    expect(html).toContain('<span class="md-kw">public</span> <span class="md-kw">function</span> show(): string');
    expect(html).toContain("<strong>Returns:</strong> <code>&lt;b&gt;</code>");
    expect(html).toContain("<ul><li>one</li><li><em>two</em></li></ul>");
  });

  it("highlights variables", () => {
    expect(highlightPhp("$x = 1")).toBe('<span class="md-var">$x</span> = 1');
  });
});

describe("structure ordering", () => {
  const n = (name: string, kind: StructureNode["kind"]): StructureNode => ({ name, kind, detail: "", modifiers: {}, offset: 0, line: 0 });
  it("sorts alphabetically within kinds only when enabled", () => {
    const nodes = [n("zeta", "method"), n("alpha", "method"), n("beta", "property"), n("MAX", "constant")];
    expect(orderNodes(nodes, false)).toBe(nodes);
    expect(orderNodes(nodes, true).map((x) => x.name)).toEqual(["MAX", "beta", "alpha", "zeta"]);
  });
});

describe("settings", () => {
  it("maps to backend language server settings", () => {
    const s = toLspSettings({ ...defaultPhpSettings, server: "intelephense", languageLevel: "8.2", includePaths: ["/usr/share/php"] });
    expect(s).toMatchObject({ server: "intelephense", licenceKey: null, phpVersion: "8.2", includePaths: ["/usr/share/php"] });
  });

  it("migrates values from the old localStorage store", () => {
    const old = JSON.stringify({ languageLevel: "8.1", cliInterpreter: "/usr/bin/php8.1", server: "phpactor", licenceKey: "SECRET", includePaths: [] });
    const changes = legacyPhpChanges(old, (k) => k === "php.lsp.server");
    expect(changes).toEqual({ "php.languageLevel": "8.1", "php.interpreter": "/usr/bin/php8.1" });
    expect(JSON.stringify(changes)).not.toContain("SECRET");
    expect(legacyPhpChanges("not json", () => false)).toEqual({});
    expect(legacyPhpChanges(null, () => false)).toEqual({});
  });

  it("computes indexing percent", () => {
    expect(percent({ done: 9, total: 20 })).toBe(45);
    expect(percent({ done: 0, total: 0 })).toBe(0);
  });
});
