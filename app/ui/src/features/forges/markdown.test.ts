import { describe, expect, it } from "vitest";
import { renderMarkdown } from "./markdown";

describe("renderMarkdown", () => {
  it("escapes HTML before adding markup", () => {
    const html = renderMarkdown('<script>alert("x")</script> **bold**');
    expect(html).not.toContain("<script>");
    expect(html).toContain("&lt;script&gt;");
    expect(html).toContain("<strong>bold</strong>");
  });

  it("renders headings, lists, task lists and code", () => {
    const html = renderMarkdown("## Summary\n\n- one\n- [x] done\n\n1. first\n\n```php\n$a = '<b>';\n```\n\nUse `isAdmin()` here.");
    expect(html).toContain("<h3>Summary</h3>");
    expect(html).toContain("<ul><li>one</li><li><input type=\"checkbox\" disabled checked> done</li></ul>");
    expect(html).toContain("<ol><li>first</li></ol>");
    expect(html).toContain('<pre class="md-code"><code>$a = &#39;&lt;b&gt;&#39;;</code></pre>');
    expect(html).toContain("<code>isAdmin()</code>");
  });

  it("links only http(s) URLs and marks references and mentions", () => {
    const html = renderMarkdown("See [docs](https://example.com/a) and [x](javascript:alert(1)), closes #28, cc @anna");
    expect(html).toContain('<a href="https://example.com/a" target="_blank" rel="noreferrer">docs</a>');
    expect(html).not.toContain('href="javascript');
    expect(html).toContain('<span class="md-ref" data-ref="28">#28</span>');
    expect(html).toContain('<span class="md-mention">@anna</span>');
  });

  it("keeps emphasis, mentions and references out of link URLs", () => {
    const html = renderMarkdown("See https://x.dev/a_b_c/~~d~~/@me#12 and [y](https://x.dev/*z*/_q_)");
    expect(html).toContain('<a href="https://x.dev/a_b_c/~~d~~/@me#12" target="_blank" rel="noreferrer">https://x.dev/a_b_c/~~d~~/@me#12</a>');
    expect(html).toContain('<a href="https://x.dev/*z*/_q_" target="_blank" rel="noreferrer">y</a>');
    expect(html).not.toContain("<em>");
  });

  it("keeps code spans literal and renders quotes", () => {
    const html = renderMarkdown("> **Note** keep `**this**`");
    expect(html).toContain("<blockquote><p><strong>Note</strong> keep <code>**this**</code></p></blockquote>");
  });
});
