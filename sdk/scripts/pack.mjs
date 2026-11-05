#!/usr/bin/env node
/**
 * Pack a Forge plugin into `<id>-<version>.forgeplugin` (a zip file): forge-plugin.json,
 * everything in dist/, the folder of each icon theme file, and the documents in
 * `DOCUMENTS` when present (README.md, LICENSE, CHANGELOG.md, and THIRD_PARTY_NOTICES.md
 * and NOTICE, so attribution travels with the package). Prints the
 * SHA-256 a marketplace index lists for it. Self-contained (Node.js only), so `forge plugin
 * new` copies it into new plugin projects.
 *
 *   node pack.mjs [plugin folder] [--out <file>]
 */
import { createHash } from "node:crypto";
import { existsSync, readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { join, relative, resolve, sep } from "node:path";
import { deflateRawSync } from "node:zlib";

const CRC_TABLE = Array.from({ length: 256 }, (_, n) => {
  let c = n;
  for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  return c >>> 0;
});

function crc32(buf) {
  let c = 0xffffffff;
  for (const b of buf) c = CRC_TABLE[(c ^ b) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

/** A zip archive of `[name, Buffer]` entries (deflated, fixed 1980-01-01 timestamps so
 * builds are reproducible). */
export function zip(entries) {
  const locals = [];
  const centrals = [];
  let offset = 0;
  for (const [name, data] of entries) {
    const nameBuf = Buffer.from(name, "utf8");
    const packed = deflateRawSync(data, { level: 9 });
    const crc = crc32(data);
    const local = Buffer.alloc(30);
    local.writeUInt32LE(0x04034b50, 0);
    local.writeUInt16LE(20, 4);
    local.writeUInt16LE(0x0800, 6); // UTF-8 names
    local.writeUInt16LE(8, 8); // deflate
    local.writeUInt16LE(0, 10);
    local.writeUInt16LE(0x21, 12); // 1980-01-01
    local.writeUInt32LE(crc, 14);
    local.writeUInt32LE(packed.length, 18);
    local.writeUInt32LE(data.length, 22);
    local.writeUInt16LE(nameBuf.length, 26);
    local.writeUInt16LE(0, 28);
    const central = Buffer.alloc(46);
    central.writeUInt32LE(0x02014b50, 0);
    central.writeUInt16LE(20, 4);
    central.writeUInt16LE(20, 6);
    central.writeUInt16LE(0x0800, 8);
    central.writeUInt16LE(8, 10);
    central.writeUInt16LE(0, 12);
    central.writeUInt16LE(0x21, 14);
    central.writeUInt32LE(crc, 16);
    central.writeUInt32LE(packed.length, 20);
    central.writeUInt32LE(data.length, 24);
    central.writeUInt16LE(nameBuf.length, 28);
    central.writeUInt32LE(offset, 42);
    locals.push(local, nameBuf, packed);
    centrals.push(central, nameBuf);
    offset += local.length + nameBuf.length + packed.length;
  }
  const centralSize = centrals.reduce((n, b) => n + b.length, 0);
  const end = Buffer.alloc(22);
  end.writeUInt32LE(0x06054b50, 0);
  end.writeUInt16LE(entries.length, 8);
  end.writeUInt16LE(entries.length, 10);
  end.writeUInt32LE(centralSize, 12);
  end.writeUInt32LE(offset, 16);
  return Buffer.concat([...locals, ...centrals, end]);
}

function walk(dir) {
  return readdirSync(dir)
    .sort()
    .flatMap((name) => {
      const p = join(dir, name);
      return statSync(p).isDirectory() ? walk(p) : [p];
    });
}

/** Files at the plugin root that are packed when present. */
export const DOCUMENTS = ["README.md", "LICENSE", "CHANGELOG.md", "THIRD_PARTY_NOTICES.md", "NOTICE"];

/** Pack the plugin in `dir`; returns `{ file, sha256, size }`. */
export function pack(dir, out) {
  const root = resolve(dir);
  const manifestPath = join(root, "forge-plugin.json");
  const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
  if (manifest.main && !existsSync(join(root, manifest.main))) {
    throw new Error(`${manifest.main} does not exist: build the plugin first`);
  }
  const files = ["forge-plugin.json", ...DOCUMENTS]
    .map((f) => join(root, f))
    .filter((f) => existsSync(f));
  if (existsSync(join(root, "dist"))) files.push(...walk(join(root, "dist")));
  // Icon themes: the theme file and everything in its folder (the SVGs it names).
  for (const theme of manifest.contributes?.iconThemes ?? []) {
    const dir = join(root, theme.path, "..");
    if (!existsSync(join(root, theme.path))) throw new Error(`${theme.path} does not exist`);
    if (resolve(dir) === root) throw new Error(`put ${theme.path} into a folder of its own (e.g. theme/), next to its SVGs`);
    files.push(...walk(dir));
  }
  const entries = [...new Set(files)].map((f) => [relative(root, f).split(sep).join("/"), readFileSync(f)]);
  const bytes = zip(entries);
  const file = resolve(out ?? join(root, `${manifest.id}-${manifest.version}.forgeplugin`));
  writeFileSync(file, bytes);
  return { file, sha256: createHash("sha256").update(bytes).digest("hex"), size: bytes.length };
}

if (import.meta.url === `file://${process.argv[1]}` || process.argv[1]?.endsWith("pack.mjs")) {
  const args = process.argv.slice(2);
  const outAt = args.indexOf("--out");
  const out = outAt >= 0 ? args.splice(outAt, 2)[1] : undefined;
  const { file, sha256, size } = pack(args[0] ?? ".", out);
  console.log(`wrote ${file} (${size} bytes)\nsha256 ${sha256}`);
}
