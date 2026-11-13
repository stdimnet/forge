import { describe, expect, it } from "vitest";
import { imageSource, lineAnchor, linkTarget, normalizePath, resolveRelative } from "./links";

describe("markdown links", () => {
  it("resolves relative paths against the document", () => {
    expect(resolveRelative("docs/guide/intro.md", "../api.md")).toBe("docs/api.md");
    expect(resolveRelative("README.md", "./app/Models/User.php")).toBe("app/Models/User.php");
    expect(resolveRelative("docs/a.md", "/composer.json")).toBe("composer.json");
    expect(resolveRelative("docs/a.md", "my%20file.md")).toBe("docs/my file.md");
    expect(resolveRelative("a.md", "../../etc/passwd")).toBeNull();
    expect(normalizePath("a/./b/../c")).toBe("a/c");
  });

  it("classifies link targets", () => {
    expect(linkTarget("README.md", "#install")).toEqual({ kind: "anchor", anchor: "install" });
    expect(linkTarget("README.md", "https://forge.dev")).toEqual({ kind: "external", url: "https://forge.dev" });
    expect(linkTarget("README.md", "mailto:a@b.c")).toEqual({ kind: "external", url: "mailto:a@b.c" });
    expect(linkTarget("README.md", "javascript:alert(1)")).toEqual({ kind: "none" });
    expect(linkTarget("docs/a.md", "b.md#order-flow")).toEqual({ kind: "file", path: "docs/b.md", anchor: "order-flow" });
    expect(linkTarget("docs/a.md", "../app/X.php#L12")).toEqual({ kind: "file", path: "app/X.php", anchor: "L12" });
    expect(linkTarget("docs/a.md", "./#top")).toEqual({ kind: "anchor", anchor: "top" });
    expect(lineAnchor("L12-L20")).toBe(12);
    expect(lineAnchor("install")).toBeUndefined();
  });

  it("finds image sources", () => {
    expect(imageSource("docs/a.md", "img/x.png")).toEqual({ kind: "file", path: "docs/img/x.png" });
    expect(imageSource("a.md", "https://img.shields.io/badge.svg")).toEqual({ kind: "url", url: "https://img.shields.io/badge.svg" });
    expect(imageSource("a.md", "data:image/png;base64,AAAA")).toEqual({ kind: "url", url: "data:image/png;base64,AAAA" });
    expect(imageSource("a.md", "data:text/html,<script>")).toEqual({ kind: "none" });
    expect(imageSource("a.md", "javascript:x")).toEqual({ kind: "none" });
  });
});
