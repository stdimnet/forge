import { describe, expect, it } from "vitest";
import type { HoverDoc, MdBlock } from "@/ipc/markdown";
import { callWasm, forgeWasm } from "@/wasm";
import { appendBlocks } from "./MarkdownDoc";
import { buildHoverDom } from "./hoverDoc";

/** Parse with the Rust `forge-markdown` crate and render with the Solid components. */
async function render(md: string): Promise<HTMLElement> {
  const wasm = await forgeWasm();
  const div = document.createElement("div");
  appendBlocks(div, await callWasm<MdBlock[]>(wasm.markdown_parse, { text: md, flavor: "docs" }));
  return div;
}

describe("documentation markdown (Rust AST, Solid rendering)", () => {
  it("never interprets raw HTML", async () => {
    const root = await render('Hello <script>alert(1)</script><img src=x onerror="alert(2)"> <b>bold</b>');
    expect(root.querySelector("script, img, b")).toBeNull();
    expect(root.textContent).toBe("Hello alert(1) bold");
  });

  it("keeps line breaks from <br>, renders inline markup and inert links", async () => {
    const root = await render("**strong** _em_ ~~old~~ `code`<br>next [docs](javascript:alert(1)) https://php.net/x");
    expect(root.querySelector("strong")?.textContent).toBe("strong");
    expect(root.querySelector("del")?.textContent).toBe("old");
    expect(root.querySelector("br")).not.toBeNull();
    expect(root.querySelector("a")).toBeNull();
    expect([...root.querySelectorAll(".md-link")].map((l) => l.textContent)).toEqual(["docs", "https://php.net/x"]);
  });

  it("marks class names and highlights fences", async () => {
    const root = await render("returns \\App\\Models\\Order and snake_case_name\n\n```php\n$a = 1;\n```");
    expect(root.querySelector(".md-fqcn")?.textContent).toBe("\\App\\Models\\Order");
    expect(root.querySelector("em")).toBeNull();
    expect(root.querySelector("pre code .tok-variable")?.textContent).toBe("$a");
  });

  it("builds a hover popup with problems on top and a struck-through deprecated signature", async () => {
    const wasm = await forgeWasm();
    const md = "__App\\Models\\User::orders__\n\n```php\n<?php\npublic function orders(): HasMany { }\n```\n\n_@deprecated_ Use `items`.";
    const doc = await callWasm<HoverDoc>(wasm.markdown_hover, { markdown: md });
    const dom = buildHoverDom(doc, [
      { range: { start: { line: 0, character: 0 }, end: { line: 0, character: 1 } }, severity: "error", message: "Boom" },
    ]);
    expect(dom.querySelector(".lsp-hover-problem")?.textContent).toBe("Boom");
    expect(dom.querySelector(".lsp-hover-signature")?.classList.contains("deprecated")).toBe(true);
    expect(dom.querySelector(".lsp-hover-definition .md-fqcn")?.textContent).toBe("\\App\\Models\\User");
    expect(dom.querySelector(".deprecated-note code")?.textContent).toBe("items");
  });
});
