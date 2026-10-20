import { describe, expect, it } from "vitest";
import { renderInline, renderMarkdown } from "./markdown";
import { buildHoverDom, parseHover } from "./hoverDoc";

const html = (frag: Node) => {
  const div = document.createElement("div");
  div.appendChild(frag);
  return div;
};

describe("markdown renderer", () => {
  it("never interprets raw HTML", () => {
    const root = html(renderMarkdown('Hello <script>alert(1)</script><img src=x onerror="alert(2)"> <b>bold</b>'));
    expect(root.querySelector("script, img, b")).toBeNull();
    expect(root.textContent).toBe("Hello alert(1) bold");
  });

  it("keeps line breaks from <br> and renders inline markup", () => {
    const root = html(renderMarkdown("**strong** _em_ ~~old~~ `code`<br>next"));
    expect(root.querySelector("strong")?.textContent).toBe("strong");
    expect(root.querySelector("em")?.textContent).toBe("em");
    expect(root.querySelector("del")?.textContent).toBe("old");
    expect(root.querySelector("code")?.textContent).toBe("code");
    expect(root.querySelector("br")).not.toBeNull();
  });

  it("renders links as inert link-styled text", () => {
    const root = html(renderMarkdown("[docs](javascript:alert(1)) https://php.net/x"));
    expect(root.querySelector("a")).toBeNull();
    const links = root.querySelectorAll(".md-link");
    expect(links[0].textContent).toBe("docs");
    expect(links[1].textContent).toBe("https://php.net/x");
  });

  it("marks fully qualified class names and keeps namespace backslashes", () => {
    const p = document.createElement("p");
    renderInline(p, "returns \\App\\Models\\Order and snake_case_name");
    expect(p.querySelector(".md-fqcn")?.textContent).toBe("\\App\\Models\\Order");
    expect(p.textContent).toBe("returns \\App\\Models\\Order and snake_case_name");
    expect(p.querySelector("em")).toBeNull();
  });

  it("supports fences, lists, headings and rules", () => {
    const root = html(renderMarkdown("# Title\n\n- a\n- b\n\n1. one\n\n---\n\n```php\n$a = 1;\n```"));
    expect(root.querySelector(".md-h1")?.textContent).toBe("Title");
    expect(root.querySelectorAll("ul li")).toHaveLength(2);
    expect(root.querySelectorAll("ol li")).toHaveLength(1);
    expect(root.querySelector("hr")).not.toBeNull();
    const code = root.querySelector("pre code")!;
    expect(code.textContent).toBe("$a = 1;");
    expect(code.querySelector(".tok-variable")?.textContent).toBe("$a");
  });
});

const INTELEPHENSE_HOVER = [
  "__Illuminate\\Database\\Eloquent\\Concerns\\HasRelationships::hasMany__",
  "",
  "```php",
  "<?php",
  "public function hasMany(string $related): HasMany { }",
  "```",
  "",
  "Define a one-to-many relationship.",
  "",
  "_@param_ `class-string` `$related`  ",
  "_@return_ `HasMany`  ",
  "_@deprecated_ Use `hasOne` instead.",
].join("\n");

describe("hover documentation", () => {
  it("splits owner, signature, description and tag sections", () => {
    const doc = parseHover(INTELEPHENSE_HOVER);
    expect(doc.definition).toBe("Illuminate\\Database\\Eloquent\\Concerns\\HasRelationships");
    expect(doc.signature).toBe("public function hasMany(string $related): HasMany");
    expect(doc.body).toBe("Define a one-to-many relationship.");
    expect(doc.sections.map((s) => s.title)).toEqual(["Params:", "Returns:"]);
    expect(doc.sections[0].entries).toEqual(["`class-string` `$related`"]);
    expect(doc.deprecated).toBe("Use `hasOne` instead.");
  });

  it("shows the namespace for classes and nothing for global functions", () => {
    expect(parseHover("__App\\Models\\User__").definition).toBe("App\\Models");
    expect(parseHover("__array_map__\n\ntext").definition).toBe("");
  });

  it("builds a popup with problems on top and a struck-through deprecated signature", () => {
    const dom = buildHoverDom(INTELEPHENSE_HOVER, [
      { range: { start: { line: 0, character: 0 }, end: { line: 0, character: 1 } }, severity: "error", message: "Boom" },
    ]);
    expect(dom.querySelector(".lsp-hover-problem")?.textContent).toBe("Boom");
    expect(dom.querySelector(".lsp-hover-signature")?.classList.contains("deprecated")).toBe(true);
    expect(dom.querySelector(".lsp-hover-definition .md-fqcn")?.textContent).toBe(
      "\\Illuminate\\Database\\Eloquent\\Concerns\\HasRelationships",
    );
    expect(dom.querySelectorAll(".lsp-hover-section-title")[0].textContent).toBe("Deprecated:");
  });
});
