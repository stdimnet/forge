import { describe, expect, it } from "vitest";
import { createIgnoreMatcher } from "./gitignoreMatcher";

const matcher = (files: Record<string, string>) => createIgnoreMatcher((dir) => files[dir ? `${dir}/.gitignore` : ".gitignore"]);

describe("gitignore matcher", () => {
  it("applies root patterns", () => {
    const m = matcher({ ".gitignore": "/vendor\n*.log\n.env\n" });
    expect(m.isIgnored("vendor", true)).toBe(true);
    expect(m.isIgnored("debug.log", false)).toBe(true);
    expect(m.isIgnored("storage/app.log", false)).toBe(true);
    expect(m.isIgnored(".env", false)).toBe(true);
    expect(m.isIgnored(".env.example", false)).toBe(false);
    expect(m.isIgnored("src", true)).toBe(false);
  });

  it("anchors patterns with a slash", () => {
    const m = matcher({ ".gitignore": "/vendor\nstorage/logs\n" });
    expect(m.isIgnored("sub/vendor", true)).toBe(false);
    expect(m.isIgnored("storage/logs", true)).toBe(true);
    expect(m.isIgnored("a/storage/logs", true)).toBe(false);
  });

  it("directory-only patterns skip files", () => {
    const m = matcher({ ".gitignore": "build/\n" });
    expect(m.isIgnored("build", true)).toBe(true);
    expect(m.isIgnored("build", false)).toBe(false);
  });

  it("deeper gitignore overrides shallower and stays local", () => {
    const m = matcher({ ".gitignore": "*.gen.php\n", "src/.gitignore": "!keep.gen.php\n/local.php\n" });
    expect(m.isIgnored("src/other.gen.php", false)).toBe(true);
    expect(m.isIgnored("src/keep.gen.php", false)).toBe(false);
    expect(m.isIgnored("src/local.php", false)).toBe(true);
    expect(m.isIgnored("local.php", false)).toBe(false);
  });

  it("everything inside an ignored directory is ignored", () => {
    const m = matcher({ ".gitignore": "var/\n!var/keep.txt\n" });
    expect(m.isIgnored("var/keep.txt", false)).toBe(true);
    expect(m.isIgnored("var/cache/x.php", false)).toBe(true);
  });

  it("supports ** and character classes", () => {
    const m = matcher({ ".gitignore": "**/cache/*.tmp\nlogs/**\nfile[0-9].txt\n" });
    expect(m.isIgnored("a/b/cache/x.tmp", false)).toBe(true);
    expect(m.isIgnored("cache/x.tmp", false)).toBe(true);
    expect(m.isIgnored("logs/a/b.txt", false)).toBe(true);
    expect(m.isIgnored("file3.txt", false)).toBe(true);
    expect(m.isIgnored("fileX.txt", false)).toBe(false);
  });

  it("skips comments and blank lines", () => {
    const m = matcher({ ".gitignore": "# *.php\n\n" });
    expect(m.isIgnored("a.php", false)).toBe(false);
  });
});
