import { describe, expect, it } from "vitest";
import { EditorState } from "@codemirror/state";
import { positionAt } from "./yamlSchema";
import "./symfony";
import "./yaml";
import { childKeys, envNames, parameterNames, readProject, referenceClass, serviceIds, tagNameTyped, triggerAt } from "./symfony";

const SERVICES = `parameters:
    app.locale: en
    'app.name': Shop

services:
    _defaults:
        autowire: true

    App\\:
        resource: '../src/'

    App\\Service\\Mailer:
        arguments: ['@logger', '%app.locale%']
    app.alias:
        alias: App\\Service\\Mailer
`;

describe("symfony services helpers", () => {
  it("reads keys under a top-level section", () => {
    expect(childKeys(SERVICES, "parameters")).toEqual(["app.locale", "app.name"]);
    expect(childKeys(SERVICES, "missing")).toEqual([]);
  });

  it("lists service ids without defaults and namespace prefixes", () => {
    expect(serviceIds(SERVICES)).toEqual(["App\\Service\\Mailer", "app.alias"]);
    expect(parameterNames(SERVICES)).toContain("app.locale");
  });

  it("reads env variable names", () => {
    expect(envNames("APP_ENV=dev\n# c\nDATABASE_URL=\"x\"\n")).toEqual(["APP_ENV", "DATABASE_URL"]);
  });

  it("recognises what is being typed", () => {
    expect(triggerAt("        arguments: ['@lo")).toEqual({ kind: "service", typed: "lo" });
    expect(triggerAt("    - '@?App\\Ser")).toEqual({ kind: "service", typed: "App\\Ser" });
    expect(triggerAt("    x: '%app.")).toEqual({ kind: "param", typed: "app." });
    expect(triggerAt("    x: '%env(")).toEqual({ kind: "env", typed: "" });
    expect(triggerAt("    x: '%env(string:DAT")).toEqual({ kind: "env", typed: "DAT" });
    expect(triggerAt("    class: App\\Ser")).toEqual({ kind: "class", typed: "App\\Ser", ids: false });
    expect(triggerAt("    parent: ")).toEqual({ kind: "class", typed: "", ids: true });
    expect(triggerAt("    me@example.com")).toBeNull();
    expect(triggerAt("    autowire: ")).toBeNull();
  });

  it("completes tag names inside tags only", () => {
    const doc = "services:\n    App\\A:\n        tags:\n            - { name: kern";
    const state = EditorState.create({ doc });
    const at = positionAt(state, doc.length);
    expect(tagNameTyped("            - { name: kern", at)).toBe("kern");
    const other = EditorState.create({ doc: "services:\n    App\\A:\n        class: x" });
    expect(tagNameTyped("        name: x", positionAt(other, other.doc.length))).toBeNull();
  });

  it("styles references", () => {
    expect(referenceClass("@logger")).toBe("cm-sf-service");
    expect(referenceClass("%app.x%")).toBe("cm-sf-param");
    expect(referenceClass("%env(FOO)%")).toBe("cm-sf-env");
    expect(referenceClass("!tagged_iterator")).toBe("cm-sf-tag");
  });

  it("tolerates IO failures", async () => {
    const failing = { listDir: () => Promise.reject(new Error("x")), readFile: () => Promise.reject(new Error("x")) };
    expect(await readProject(failing)).toEqual({ files: [], env: [".env", ".env.local", ".env.dev", ".env.test", ".env.prod", ".env.dist", ".env.example"].map(() => "") });
    const fs = {
      listDir: (d: string) =>
        Promise.resolve(d === "config" ? [{ name: "services.yaml", path: "config/services.yaml", kind: "file" as const }, { name: "x.php", path: "config/x.php", kind: "file" as const }] : []),
      readFile: (p: string) => (p === ".env" ? Promise.resolve("A=1") : p === "config/services.yaml" ? Promise.resolve(SERVICES) : Promise.reject(new Error("no"))),
    };
    const data = await readProject(fs);
    expect(data.files).toEqual([SERVICES]);
    expect(data.env[0]).toBe("A=1");
  });
});
