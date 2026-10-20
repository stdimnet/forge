/**
 * Forge's version (Pride Versioning, see versionBump.mjs), kept the same in Cargo.toml,
 * tauri.conf.json and package.json.
 *
 *   pnpm version:check                         fail when the files disagree
 *   pnpm version:bump proud|default|shame      write the next version everywhere
 *   pnpm version:bump default --pre beta.1     … as a pre-release (beta channel)
 *
 * It never commits, tags or publishes; it prints the tag for the GitHub release.
 */
import { execFileSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { VERSION_FILES, bumpVersion, readVersion, writeVersion } from "./versionBump.mjs";

const root = resolve(import.meta.dirname, "../../..");
const read = (f) => readFileSync(resolve(root, f), "utf8");

function current() {
  const found = VERSION_FILES.map((f) => [f, readVersion(f, read(f))]);
  const versions = new Set(found.map(([, v]) => v));
  if (versions.size !== 1) {
    for (const [f, v] of found) console.error(`  ${f}: ${v}`);
    throw new Error("the version files disagree; run `pnpm version:bump` or fix them by hand");
  }
  return found[0][1];
}

const [command, part, ...rest] = process.argv.slice(2);
try {
  if (command === "check") {
    console.log(`version ${current()} (${VERSION_FILES.join(", ")})`);
  } else if (command === "bump") {
    const preAt = rest.indexOf("--pre");
    const pre = preAt >= 0 ? rest[preAt + 1] : null;
    if (preAt >= 0 && !pre) throw new Error("--pre needs a label, e.g. --pre beta.1");
    const from = current();
    const to = bumpVersion(from, /** @type {any} */ (part), pre);
    for (const f of VERSION_FILES) writeFileSync(resolve(root, f), writeVersion(f, read(f), to));
    // Cargo.lock records the workspace crates' versions.
    execFileSync("cargo", ["update", "--workspace", "--offline"], { cwd: root, stdio: "inherit" });
    console.log(`${from} → ${to}\nCommit the change, then publish a GitHub release tagged v${to}.`);
  } else {
    throw new Error("usage: version.mjs check | bump proud|default|shame [--pre <label>]");
  }
} catch (e) {
  console.error(`error: ${e instanceof Error ? e.message : e}`);
  process.exit(1);
}
