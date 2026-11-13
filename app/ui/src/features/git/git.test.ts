import { describe, expect, it } from "vitest";
import { Text } from "@codemirror/state";
import { Chunk } from "@codemirror/merge";
import { layoutGraph } from "./graph";
import { lineDiff, unifiedDiff } from "./textDiff";
import { lineAlignedDiff, lineDiffConfig } from "./lineDiff";
import { buildCommitPrompt, cleanCommitMessage, truncateDiff } from "./aiMessage";
import { ageBucket, timeAgo } from "./format";
import { displayStatus } from "./api";

describe("commit graph layout", () => {
  it("keeps a linear history in one lane", () => {
    const rows = layoutGraph([
      { sha: "c", parents: ["b"] },
      { sha: "b", parents: ["a"] },
      { sha: "a", parents: [] },
    ]);
    expect(rows.map((r) => r.lane)).toEqual([0, 0, 0]);
    expect(rows.every((r) => r.width === 1)).toBe(true);
  });

  it("opens a lane for a merge and closes it at the fork point", () => {
    // m merges x into y; both come from base.
    const rows = layoutGraph([
      { sha: "m", parents: ["y", "x"] },
      { sha: "y", parents: ["base"] },
      { sha: "x", parents: ["base"] },
      { sha: "base", parents: [] },
    ]);
    expect(rows.map((r) => r.lane)).toEqual([0, 0, 1, 0]);
    expect(rows[0].lines.some((l) => l.x1 === 0 && l.x2 === 1 && l.y2 === 1)).toBe(true);
    // x's lane converges into base's lane.
    expect(rows[3].lines.some((l) => l.x1 === 1 && l.x2 === 0)).toBe(true);
    expect(rows[1].lines.some((l) => l.x1 === 1 && l.x2 === 1)).toBe(true);
    expect(rows[0].color).not.toBe(rows[2].color);
  });

  it("ignores parents outside the loaded page", () => {
    const rows = layoutGraph([{ sha: "a", parents: ["missing"] }]);
    expect(rows[0].lines).toEqual([]);
  });
});

describe("text diffs", () => {
  it("computes a minimal line edit script", () => {
    const ops = lineDiff("a\nb\nc\n", "a\nx\nc\nd\n");
    expect(ops.filter((o) => o.kind === "same").map((o) => o.text)).toEqual(["a", "c"]);
    expect(ops.filter((o) => o.kind === "add").map((o) => o.text)).toEqual(["x", "d"]);
    expect(ops.filter((o) => o.kind === "del").map((o) => o.text)).toEqual(["b"]);
  });

  it("renders git-style unified diffs", () => {
    const d = unifiedDiff("a.php", "1\n2\n3\n", "1\n2\n4\n");
    expect(d).toContain("--- a/a.php\n+++ b/a.php");
    expect(d).toContain("@@ -1,3 +1,3 @@");
    expect(d).toContain("-3\n+4");
    expect(unifiedDiff("n.php", null, "x\n")).toContain("new file mode");
    expect(unifiedDiff("same.php", "x", "x")).toBe("");
  });

  it("aligns inserted methods to whole lines", () => {
    const a = "class A\n{\n    public function a() {}\n\n    protected function b() {}\n}\n";
    const b = "class A\n{\n    public function a() {}\n\n    public function c() {}\n\n    protected function b() {}\n}\n";
    const chunks = Chunk.build(Text.of(a.split("\n")), Text.of(b.split("\n")), lineDiffConfig);
    expect(chunks).toHaveLength(1);
    expect(chunks[0].fromA).toBe(chunks[0].toA); // pure insertion
    const changes = lineAlignedDiff("x\nold line\ny\n", "x\nnew line\ny\n");
    expect(changes.length).toBeGreaterThan(0);
    for (const c of changes) expect(c.fromA).toBeGreaterThanOrEqual(2);
  });
});

describe("AI commit message", () => {
  it("builds a prompt with files and diff", () => {
    const p = buildCommitPrompt("diff --git a/x b/x\n+1\n", [{ path: "x", status: "modified" }], "main");
    expect(p).toContain("Current branch: main");
    expect(p).toContain("modified  x");
    expect(p).toContain("+1");
  });

  it("truncates large diffs per file", () => {
    const big = Array.from({ length: 10 }, (_, i) => `diff --git a/f${i} b/f${i}\n${"+x\n".repeat(3000)}`).join("");
    const t = truncateDiff(big, 5000);
    expect(t.length).toBeLessThan(5600);
    expect(t).toContain("omitted");
    expect(t).toContain("diff --git a/f0");
  });

  it("cleans model replies", () => {
    expect(cleanCommitMessage("```\nFix login redirect.\n```")).toBe("Fix login redirect");
    expect(cleanCommitMessage('"Add tests"')).toBe("Add tests");
    expect(cleanCommitMessage("Commit message: Add x\n\nBody text")).toBe("Add x\n\nBody text");
    const long = cleanCommitMessage(`Add ${"very ".repeat(30)}long subject`);
    expect(long.length).toBeLessThanOrEqual(72);
  });
});

describe("formatting", () => {
  it("formats relative times like PHPStorm", () => {
    const now = Date.UTC(2026, 9, 20);
    expect(timeAgo(now / 1000 - 2 * 86400, now)).toBe("2 days ago");
    expect(timeAgo(now / 1000 - 3600, now)).toBe("1 hour ago");
    expect(timeAgo(now / 1000, now)).toBe("moments ago");
  });

  it("buckets blame ages", () => {
    expect(ageBucket(100, 0, 100)).toBe(0);
    expect(ageBucket(0, 0, 100)).toBe(4);
    expect(ageBucket(5, 5, 5)).toBe(0);
  });

  it("maps porcelain states to display colors", () => {
    const e = { path: "a", origPath: null, conflict: false } as const;
    expect(displayStatus({ ...e, index: "untracked", worktree: "untracked" })).toBe("untracked");
    expect(displayStatus({ ...e, index: "added", worktree: "modified" })).toBe("added");
    expect(displayStatus({ ...e, index: "unmodified", worktree: "deleted" })).toBe("deleted");
    expect(displayStatus({ ...e, index: "modified", worktree: "unmodified" })).toBe("modified");
    expect(displayStatus({ ...e, index: "conflicted", worktree: "conflicted", conflict: true })).toBe("conflict");
  });
});
