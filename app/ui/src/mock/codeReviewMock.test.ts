import { describe, expect, it } from "vitest";
import "./workspaceMock";
import "./gitMock";
import "./forgesMock";
import "./aiMock";
import "./aiEditorMock";
import "./codeReviewMock";
import { reviewApi } from "@/ipc/codeReview";
import { forgesApi } from "@/ipc/forges";
import { setSetting } from "./settingsMock";
import { mockFiles } from "./workspaceMock";

describe("browser-mode review mode", () => {
  it("keeps the viewed state per file", async () => {
    const s = await reviewApi.session(41);
    expect(s.files.every((f) => !f.viewed)).toBe(true);
    const f = s.files[0];
    await reviewApi.setViewed(41, f.path, f.digest, true);
    expect((await reviewApi.session(41)).files[0].viewed).toBe(true);
  });

  it("summarizes, reviews and turns a finding into a pending comment", async () => {
    const sum = await reviewApi.summarize(41);
    expect(sum.summary.testPlan.length).toBeGreaterThan(0);
    expect(sum.provider).toBeTruthy();
    const fv = await reviewApi.aiReview(41);
    expect(fv.findings.length).toBe(4);
    expect(fv.findings.every((f) => f.position?.newLine)).toBe(true);
    const u = await reviewApi.finding(41, fv.findings[0].id, "draft");
    expect(u.drafts).toHaveLength(1);
    expect(u.findings?.findings).toHaveLength(3);
    // Cached for the head commit.
    expect((await reviewApi.session(41)).summary?.headSha).toBe(sum.headSha);
  });

  it("submits pending comments as one review", async () => {
    await reviewApi.submit(41, "comment", "Thanks!");
    expect((await reviewApi.session(41)).drafts).toHaveLength(0);
    const bodies = (await forgesApi.discussions(41)).flatMap((d) => d.notes.map((n) => n.body));
    expect(bodies).toContain("Thanks!");
    expect(bodies.some((b) => b.startsWith("**Error: Unknown role values throw**"))).toBe(true);
    await expect(reviewApi.submit(41, "comment", " ")).rejects.toThrow(/Add a comment/);
  });

  it("sends nothing for files in .aiignore or with AI off for the project", async () => {
    mockFiles[".aiignore"] = "tests/\n";
    const s = await reviewApi.session(41);
    expect(s.files.find((f) => f.path === "tests/Unit/UserTest.php")?.aiExcluded).toBe(true);
    const fv = await reviewApi.aiReview(41, true);
    expect(fv.excluded).toContain("tests/Unit/UserTest.php");
    expect(fv.findings.some((f) => f.path.startsWith("tests/"))).toBe(false);
    await expect(reviewApi.explain(41, "tests/Unit/UserTest.php", 0)).rejects.toThrow(/aiignore/);
    delete mockFiles[".aiignore"];
    setSetting("ai.enabled", false, "project");
    expect((await reviewApi.session(41)).ai.problem).toMatch(/turned off/);
    await expect(reviewApi.summarize(41, true)).rejects.toThrow(/turned off/);
    setSetting("ai.enabled", true, "project");
  });
});
