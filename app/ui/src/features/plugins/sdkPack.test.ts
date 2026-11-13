import { afterAll, describe, expect, it } from "vitest";
import { DOCUMENTS, pack } from "../../../../../sdk/scripts/pack.mjs";

/** Node built-ins: vitest runs in Node (app/ui has no @types/node). */
type Fs = {
  mkdtempSync(prefix: string): string;
  mkdirSync(path: string, opts: { recursive: boolean }): void;
  writeFileSync(path: string, data: string): void;
  readFileSync(path: string): Uint8Array;
  rmSync(path: string, opts: { recursive: boolean; force: boolean }): void;
};
const proc = (globalThis as unknown as { process: { getBuiltinModule(id: string): unknown } }).process;
const fs = proc.getBuiltinModule("node:fs") as Fs;
const os = proc.getBuiltinModule("node:os") as { tmpdir(): string };

/** The file names in a zip's central directory. */
function zipNames(bytes: Uint8Array): string[] {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const end = bytes.byteLength - 22;
  let at = view.getUint32(end + 16, true);
  const names: string[] = [];
  for (let i = 0; i < view.getUint16(end + 10, true); i++) {
    const nameLen = view.getUint16(at + 28, true);
    const extra = view.getUint16(at + 30, true) + view.getUint16(at + 32, true);
    names.push(new TextDecoder().decode(bytes.subarray(at + 46, at + 46 + nameLen)));
    at += 46 + nameLen + extra;
  }
  return names;
}

const dirs: string[] = [];
afterAll(() => dirs.forEach((d) => fs.rmSync(d, { recursive: true, force: true })));

function plugin(files: Record<string, string>): string {
  const dir = fs.mkdtempSync(`${os.tmpdir()}/forge-pack-`);
  dirs.push(dir);
  const manifest = { id: "acme.notices", name: "Notices", version: "1.0.0", publisher: "Acme", engines: { forge: ">=0.1.0" }, main: "dist/plugin.js" };
  fs.writeFileSync(`${dir}/forge-plugin.json`, JSON.stringify(manifest));
  fs.mkdirSync(`${dir}/dist`, { recursive: true });
  fs.writeFileSync(`${dir}/dist/plugin.js`, "var __forgePlugin = {};");
  for (const [name, text] of Object.entries(files)) fs.writeFileSync(`${dir}/${name}`, text);
  return dir;
}

describe("sdk pack", () => {
  it("packs the attribution files with the plugin", () => {
    const dir = plugin({ "README.md": "# Notices", LICENSE: "MIT", "THIRD_PARTY_NOTICES.md": "Icons: MIT", NOTICE: "Acme", "notes.txt": "not packed" });
    const { file, size } = pack(dir, `${dir}/out.forgeplugin`);
    const bytes = fs.readFileSync(file);
    expect(bytes.byteLength).toBe(size);
    expect(zipNames(bytes).sort()).toEqual(["LICENSE", "NOTICE", "README.md", "THIRD_PARTY_NOTICES.md", "dist/plugin.js", "forge-plugin.json"]);
    expect(DOCUMENTS).toEqual(expect.arrayContaining(["THIRD_PARTY_NOTICES.md", "NOTICE"]));
  });

  it("skips documents that are not there", () => {
    const dir = plugin({ "CHANGELOG.md": "## 1.0.0" });
    const names = zipNames(fs.readFileSync(pack(dir, `${dir}/out.forgeplugin`).file));
    expect(names.sort()).toEqual(["CHANGELOG.md", "dist/plugin.js", "forge-plugin.json"]);
  });
});
