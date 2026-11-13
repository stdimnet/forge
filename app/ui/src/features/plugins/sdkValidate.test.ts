import { afterAll, describe, expect, it } from "vitest";
import { basicChecks, findForge, isForge, validate } from "../../../../../sdk/scripts/validate.mjs";

/** Node built-ins: vitest runs in Node (app/ui has no @types/node). */
type Fs = {
  mkdtempSync(prefix: string): string;
  mkdirSync(path: string, opts: { recursive: boolean }): void;
  writeFileSync(path: string, data: string): void;
  rmSync(path: string, opts: { recursive: boolean; force: boolean }): void;
};
const proc = (globalThis as unknown as { process: { getBuiltinModule(id: string): unknown } }).process;
const fs = proc.getBuiltinModule("node:fs") as Fs;
const os = proc.getBuiltinModule("node:os") as { tmpdir(): string };

const dirs: string[] = [];
afterAll(() => dirs.forEach((d) => fs.rmSync(d, { recursive: true, force: true })));

function plugin(manifest: Record<string, unknown>, files: Record<string, string> = {}): string {
  const dir = fs.mkdtempSync(`${os.tmpdir()}/forge-validate-`);
  dirs.push(dir);
  fs.writeFileSync(`${dir}/forge-plugin.json`, JSON.stringify(manifest));
  for (const [name, text] of Object.entries(files)) {
    if (name.includes("/")) fs.mkdirSync(`${dir}/${name.slice(0, name.lastIndexOf("/"))}`, { recursive: true });
    fs.writeFileSync(`${dir}/${name}`, text);
  }
  return dir;
}

const GOOD = { manifestVersion: 1, id: "acme.hello", name: "Hello", version: "1.0.0", engines: { forge: ">=0.1.0" }, main: "dist/plugin.js" };

describe("sdk validate", () => {
  it("finds no Forge binary where there is none", () => {
    const empty = fs.mkdtempSync(`${os.tmpdir()}/forge-nobin-`);
    dirs.push(empty);
    expect(isForge(undefined)).toBe(false);
    expect(isForge(`${empty}/forge`)).toBe(false);
    expect(findForge({ env: { FORGE_BIN: `${empty}/forge`, PATH: empty } })).toBeNull();
  });

  it("runs basic checks without the binary", () => {
    expect(basicChecks(plugin(GOOD, { "dist/plugin.js": "var __forgePlugin = {};" }))).toEqual([]);
    const bad = basicChecks(plugin({ ...GOOD, id: "Bad Id", version: "one", engines: undefined }));
    expect(bad).toEqual([
      "id: must be a lowercase reverse-DNS id like `publisher.plugin-name`",
      "version: must be a SemVer version like `1.0.0`",
      "engines.forge: must be a version range like `>=0.1.0`",
      "main: dist/plugin.js does not exist (build the plugin first)",
    ]);
  });

  it("says that only basic checks ran", () => {
    const logs: string[] = [];
    const errors: string[] = [];
    const io = { log: (m: string) => logs.push(m), error: (m: string) => errors.push(m) };
    const env = proc as unknown as { env: Record<string, string | undefined> };
    const keys = ["FORGE_BIN", "PATH", "CARGO_TARGET_DIR"];
    const saved = keys.map((k) => env.env[k]);
    for (const k of keys) env.env[k] = "";
    try {
      expect(validate([plugin(GOOD)], io)).toBe(1);
      expect(errors[0]).toContain("Forge binary not found");
      expect(logs.at(-1)).toBe("invalid (1 basic problem)");
      expect(validate(["x.forgeplugin"], io)).toBe(2);
      expect(validate([], io)).toBe(2);
    } finally {
      keys.forEach((k, i) => {
        if (saved[i] === undefined) delete env.env[k];
        else env.env[k] = saved[i];
      });
    }
  });
});
