import { describe, expect, it } from "vitest";
import type { ServerRow } from "@/ipc/langservers";
import type { ServerStatus } from "@/ipc/lsp";
import { shortName, summarize } from "@/features/lsp/LspStatusItem";
import { groupRows, jsonError, searchKind, shortenHome, skipText, uniqueSymbols } from "./model";
import { outcomeText } from "./InstallDialog";
import { symbolItem } from "./symbols";

const row = (id: string, group: string, extra: Partial<ServerRow> = {}): ServerRow => ({
  id,
  name: id,
  group,
  languages: [group],
  command: id,
  args: [],
  homepage: "https://example.com",
  license: "MIT",
  enabled: true,
  defaultEnabled: true,
  status: "installed",
  inProject: false,
  install: [],
  defaults: { initializationOptions: null, settings: null },
  source: "catalog",
  ...extra,
});

describe("language servers page model", () => {
  it("groups rows with project languages first and filters", () => {
    const rows = [row("gopls", "Go"), row("tls", "TypeScript", { inProject: true }), row("vtsls", "TypeScript"), row("ra", "Rust")];
    const groups = groupRows(rows);
    expect(groups.map((g) => g.title)).toEqual(["TypeScript", "Go", "Rust"]);
    expect(groups[0].rows.map((r) => r.id)).toEqual(["tls", "vtsls"]);
    expect(groupRows(rows, { filter: "rus" }).map((g) => g.title)).toEqual(["Rust"]);
    expect(groupRows(rows, { projectOnly: true }).flatMap((g) => g.rows.map((r) => r.id))).toEqual(["tls"]);
  });

  it("explains why a server does not run", () => {
    const rows = [row("typescript-language-server", "TS", { name: "TypeScript Language Server" })];
    expect(skipText({ reason: "alternative", used: "typescript-language-server" }, rows)).toBe(
      "Not started: TypeScript Language Server is used instead.",
    );
    expect(skipText({ reason: "pluginDisabled", plugin: "forge.web" })).toContain("forge.web");
    expect(skipText(null)).toBeNull();
  });

  it("formats paths, JSON errors and kinds", () => {
    expect(shortenHome("/home/marlon/.cargo/bin/rust-analyzer")).toBe("~/.cargo/bin/rust-analyzer");
    expect(shortenHome("/Users/me/go/bin/gopls")).toBe("~/go/bin/gopls");
    expect(shortenHome("C:\\Users\\me\\AppData\\Roaming\\npm\\x.cmd")).toBe("~\\AppData\\Roaming\\npm\\x.cmd");
    expect(shortenHome("/usr/bin/clangd")).toBe("/usr/bin/clangd");
    expect(jsonError("")).toBeNull();
    expect(jsonError('{"a": 1}')).toBeNull();
    expect(jsonError("[1]")).toBe("Must be a JSON object.");
    expect(jsonError("{")).not.toBeNull();
    expect(searchKind("struct")).toBe("class");
    expect(searchKind("field")).toBe("property");
    expect(searchKind("enumMember")).toBe("constant");
    expect(searchKind("namespace")).toBeUndefined();
  });

  it("describes install outcomes", () => {
    expect(outcomeText("succeeded", "gopls")).toBe("gopls was installed.");
    expect(outcomeText({ failed: { code: 1 } }, "gopls")).toContain("exit code 1");
    expect(outcomeText("cancelled", "gopls")).toContain("cancelled");
  });
});

describe("workspace symbols", () => {
  const sym = (name: string, server: string, line = 3) => ({
    name,
    kind: "interface",
    containerName: null,
    path: "resources/js/app.ts",
    range: { start: { line, character: 10 }, end: { line, character: 15 } },
    server,
  });

  it("drops duplicates reported by several servers", () => {
    expect(uniqueSymbols([sym("Order", "ts"), sym("Order", "eslint"), sym("Order", "ts", 9)])).toHaveLength(2);
  });

  it("turns symbols into Search Everywhere items", () => {
    const item = symbolItem(sym("Order", "ts"), "ord")!;
    expect(item.label).toBe("Order");
    expect(item.kind).toBe("interface");
    expect(item.hint).toBe("app.ts:4");
    expect(item.detail).toBe("resources/js");
    expect(item.matches).toEqual([0, 1, 2]);
  });
});

describe("language server status bar", () => {
  const s = (id: string, state: ServerStatus["state"], extra: object = {}) => ({ id, name: id, state, ...extra }) as ServerStatus;

  it("prefers indexing progress, then failures, then a summary", () => {
    expect(summarize([])).toBeNull();
    expect(summarize([s("a", "notStarted")])).toBeNull();
    const busy = summarize([s("a", "ready"), s("TypeScript Language Server", "busy", { message: "Indexing", percentage: 45 })]);
    expect(busy?.kind).toBe("busy");
    expect(summarize([s("a", "ready"), s("b", "failed", { error: "x" })])?.kind).toBe("failed");
    const ready = summarize([s("a", "ready"), s("b", "ready")]);
    expect(ready).toMatchObject({ kind: "ready", count: 2 });
    expect(shortName("TypeScript Language Server")).toBe("TypeScript");
    expect(shortName("Vue Language Server (Volar)")).toBe("Vue");
    expect(shortName("gopls")).toBe("gopls");
  });
});
