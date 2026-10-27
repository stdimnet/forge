import { describe, expect, it } from "vitest";
import { Text } from "@codemirror/state";
import type { SourceRootEntry } from "@/ipc/phpMeta";
import { mayComplete, toDiagnostics } from "./meta";
import { markChange, upsertRoot } from "./sourceRoots";

const composerRoot = (path: string, prefix: string, kind: "source" | "test" = "source"): SourceRootEntry => ({
  path,
  prefix,
  kind,
  origin: "composer",
  overridden: false,
});

describe("PHP metadata in the editor", () => {
  it("asks the backend only where values can be completed", () => {
    expect(mayComplete("$o->setStatus('")).toBe(true);
    expect(mayComplete("$o->sort('total', ")).toBe(true);
    expect(mayComplete("if ($o->status() === 'pa")).toBe(true);
    expect(mayComplete("$data['")).toBe(true);
    expect(mayComplete("Mailer::HTML | ")).toBe(true);
    expect(mayComplete("$total = 1")).toBe(false);
    expect(mayComplete("echo $name")).toBe(false);
  });

  it("turns problems into struck-through or greyed diagnostics", () => {
    const doc = Text.of(["<?php", "$o->settle();"]);
    const [d] = toDiagnostics(doc, [{ start: 10, end: 16, severity: "weak", message: "Method 'settle' is deprecated", code: "PhpDeprecation", deprecated: true }]);
    expect(d).toMatchObject({ range: { start: { line: 1, character: 4 }, end: { line: 1, character: 10 } }, deprecated: true, source: "PHP", code: "PhpDeprecation" });
  });
});

describe("source roots", () => {
  it("marks, changes and unmarks folders", () => {
    const roots = [composerRoot("app", "App"), composerRoot("tests", "Tests", "test")];
    expect(markChange([], roots, "modules", "source", "Modules")).toEqual([{ path: "modules", prefix: "Modules", kind: "source" }]);
    // Marking a composer.json root as it is changes nothing; as tests it does.
    expect(markChange([], roots, "app", "source", "App")).toEqual([]);
    expect(markChange([], roots, "app", "test", "App")).toEqual([{ path: "app", prefix: "App", kind: "test" }]);
    // Unmarking: composer.json roots get a `none` entry, the user's own go away.
    expect(markChange([], roots, "tests", null, "")).toEqual([{ path: "tests", prefix: "Tests", kind: "none" }]);
    const user: SourceRootEntry = { path: "modules", prefix: "Modules", kind: "source", origin: "user", overridden: false };
    expect(markChange([{ path: "modules", prefix: "Modules", kind: "source" }], [...roots, user], "modules", null, "")).toEqual([]);
    expect(upsertRoot([{ path: "a/", prefix: "A", kind: "source" }], "./a", null)).toEqual([]);
  });
});
