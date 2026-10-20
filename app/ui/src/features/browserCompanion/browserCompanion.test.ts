import { describe, expect, it } from "vitest";
import { Text } from "@codemirror/state";
import type { OpenEvent, ProjectProfile } from "@/ipc/companion";
import { offsetAt } from "@/editor/views";
import { planOpen } from "./openRequests";
import { INTEGRATIONS, isApplied, rankIntegrations } from "./integrations";
import { formatAgo, formatCountdown, formatPairingCode } from "./format";

const event = (target: OpenEvent["target"], source: OpenEvent["source"] = "browser"): OpenEvent => ({
  source,
  client: "Chrome",
  file: "/home/me/shop/src/App.tsx",
  line: 12,
  column: 3,
  target,
});

describe("planOpen", () => {
  it("opens files of the open project directly", () => {
    expect(planOpen(event({ kind: "open", root: "/home/me/shop", path: "src/App.tsx" }), "/home/me/shop")).toEqual({
      kind: "open",
      path: "src/App.tsx",
    });
  });

  it("asks before switching to a recent project", () => {
    const plan = planOpen(
      event({ kind: "recent", root: "/home/me/blog", name: "blog", path: "app/Post.php" }),
      "/home/me/shop",
    );
    expect(plan).toEqual({
      kind: "switch",
      root: "/home/me/blog",
      path: "app/Post.php",
      ask: true,
      reason: "recent",
      projectName: "blog",
    });
  });

  it("asks before opening files outside known projects, unless typed in a terminal", () => {
    const outside = { kind: "outside", suggestedRoot: "/tmp/lib", path: "x.php" } as const;
    expect(planOpen(event(outside, "link"), "/home/me/shop")).toMatchObject({ ask: true, reason: "outside", projectName: "lib" });
    expect(planOpen(event(outside, "cli"), null)).toEqual({ kind: "switch", root: "/tmp/lib", path: "x.php", ask: false });
  });

  it("asks when the project was closed after the request was resolved", () => {
    const plan = planOpen(event({ kind: "open", root: "/home/me/shop", path: "a.ts" }), null);
    expect(plan).toMatchObject({ kind: "switch", ask: true });
  });
});

const profile = (p: Partial<ProjectProfile>): ProjectProfile => ({ composer: [], npm: [], env: {}, files: [], ...p });

describe("integrations", () => {
  it("lists detected frameworks first", () => {
    const ranked = rankIntegrations(profile({ composer: ["laravel/framework"], npm: ["vite", "vue"] }));
    const detected = ranked.filter((r) => r.detected).map((r) => r.integration.id);
    expect(detected).toEqual(["laravel", "xdebug", "launch-editor"]);
    expect(ranked).toHaveLength(INTEGRATIONS.length);
    expect(ranked[detected.length].detected).toBe(false);
  });

  it("detects Angular and React projects", () => {
    const ids = (p: ProjectProfile) => rankIntegrations(p).filter((r) => r.detected).map((r) => r.integration.id);
    expect(ids(profile({ npm: ["@angular/core"] }))).toEqual(["angular"]);
    expect(ids(profile({ npm: ["react", "vite"] }))).toEqual(["launch-editor", "jsx-source"]);
    expect(ids(profile({}))).toEqual([]);
  });

  it("knows when a dotenv change is in place", () => {
    const laravel = INTEGRATIONS.find((i) => i.id === "laravel")!;
    expect(isApplied(laravel, profile({ env: { ".env": { IGNITION_EDITOR: "forge" } } }))).toBe(true);
    expect(isApplied(laravel, profile({ env: { ".env": { IGNITION_EDITOR: "phpstorm" } } }))).toBe(false);
    expect(isApplied(laravel, null)).toBe(false);
  });

  it("uses the forge:// URL in every link template", () => {
    for (const i of INTEGRATIONS.filter((i) => ["laravel", "symfony", "xdebug", "whoops"].includes(i.id))) {
      expect(i.snippets.some((s) => s.code.includes("forge://open?file=")), i.id).toBe(true);
    }
  });
});

describe("format", () => {
  it("formats codes, countdowns and relative times", () => {
    expect(formatPairingCode("482913")).toBe("482 913");
    expect(formatCountdown(299)).toBe("4:59");
    expect(formatCountdown(5)).toBe("0:05");
    expect(formatAgo(1000, 1030)).toBe("just now");
    expect(formatAgo(1000, 1000 + 120)).toBe("2 min ago");
    expect(formatAgo(0, 3 * 86400)).toBe("3 days ago");
  });
});

describe("offsetAt", () => {
  const doc = Text.of(["<?php", "", "class User", "{"]);
  it("maps 1-based positions and clamps them", () => {
    expect(offsetAt(doc, 3, 7)).toBe(doc.line(3).from + 6);
    expect(offsetAt(doc, 3)).toBe(doc.line(3).from);
    expect(offsetAt(doc, 99, 1)).toBe(doc.line(4).from);
    expect(offsetAt(doc, 1, 999)).toBe(doc.line(1).to);
    expect(offsetAt(doc, 0, 0)).toBe(0);
  });
});
