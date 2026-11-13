import { describe, expect, it } from "vitest";
import type { ServerRow } from "@/ipc/langservers";
import { activeHint as activeHintIn, type HintInput, type ServerHint } from "./hints";

/** Hints like the ones Forge had for `$schema` before its own JSON Schema support. */
const HINTS: ServerHint[] = [
  {
    serverId: "vscode-json-language-server",
    languageIds: ["json"],
    applies: (head) => /"\$schema"\s*:\s*"/.test(head),
    title: "JSON Language Server is not installed.",
    detail: "",
  },
  {
    serverId: "yaml-language-server",
    languageIds: ["yaml", "docker-compose", "symfony-services"],
    applies: (head) => /^#\s*yaml-language-server:\s*\$schema\s*=/m.test(head),
    title: "YAML Language Server is not installed.",
    detail: "",
  },
];
const activeHint = (input: HintInput) => activeHintIn(input, HINTS);

const row = (over: Partial<ServerRow> = {}): ServerRow =>
  ({
    id: "vscode-json-language-server",
    name: "JSON Language Server",
    group: "JSON",
    languages: ["json"],
    command: "vscode-json-language-server",
    args: [],
    homepage: "",
    license: "MIT",
    enabled: true,
    defaultEnabled: true,
    status: "notInstalled",
    inProject: true,
    install: [{ index: 0, tool: "npm", command: "npm install vscode-langservers-extracted", runnable: true, available: true }],
    defaults: { initializationOptions: null, settings: null },
    source: "catalog",
    ...over,
  }) as ServerRow;

const input = (over: Partial<HintInput> = {}): HintInput => ({
  languageId: "json",
  head: '{\n  "$schema": "https://docs.renovatebot.com/renovate-schema.json"\n}',
  rows: [row()],
  dismissed: [],
  enabled: true,
  ...over,
});

describe("activeHint", () => {
  it("has no built-in hints for $schema: Forge validates with it itself", () => {
    expect(activeHintIn(input())).toBeNull();
  });

  it("suggests the JSON server for a file with $schema", () => {
    const a = activeHint(input());
    expect(a?.hint.serverId).toBe("vscode-json-language-server");
    expect(a?.row.id).toBe("vscode-json-language-server");
  });

  it("stays quiet without $schema or for other file types", () => {
    expect(activeHint(input({ head: '{ "name": "x" }' }))).toBeNull();
    expect(activeHint(input({ languageId: "php" }))).toBeNull();
    expect(activeHint(input({ languageId: undefined }))).toBeNull();
    // A property that merely mentions the word is not a schema reference.
    expect(activeHint(input({ head: '{ "description": "uses $schema somehow" }' }))).toBeNull();
  });

  it("stays quiet when the server is installed, running, disabled or unknown", () => {
    for (const status of ["installed", "running", "disabled"] as const) {
      expect(activeHint(input({ rows: [row({ status })] }))).toBeNull();
    }
    expect(activeHint(input({ rows: [row({ enabled: false })] }))).toBeNull();
    expect(activeHint(input({ rows: [] }))).toBeNull();
    expect(activeHint(input({ rows: [row({ install: [] })] }))).toBeNull();
  });

  it("honours the setting and a dismissed server", () => {
    expect(activeHint(input({ enabled: false }))).toBeNull();
    expect(activeHint(input({ dismissed: ["vscode-json-language-server"] }))).toBeNull();
    expect(activeHint(input({ dismissed: ["yaml-language-server"] }))?.hint.serverId).toBe("vscode-json-language-server");
  });

  it("suggests the YAML server for the schema modeline", () => {
    const yaml = row({ id: "yaml-language-server", name: "YAML Language Server" });
    const head = "# yaml-language-server: $schema=https://example.org/schema.json\nkey: 1\n";
    expect(activeHint(input({ languageId: "yaml", head, rows: [yaml] }))?.hint.serverId).toBe("yaml-language-server");
    expect(activeHint(input({ languageId: "docker-compose", head, rows: [yaml] }))).not.toBeNull();
    expect(activeHint(input({ languageId: "yaml", head: "key: 1\n", rows: [yaml] }))).toBeNull();
  });
});
