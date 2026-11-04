/**
 * Forge's version (Pride Versioning, see versionBump.mjs), kept the same in Cargo.toml,
 * tauri.conf.json and package.json.
 *
 *   pnpm version:check                         fail when the files disagree or the version has
 *                                              no MSI version (versionBump.mjs msiVersion)
 *   pnpm version:bump proud|default|shame      write the next version everywhere
 *   pnpm version:bump default --pre beta.1     … as a pre-release (beta channel)
 *   pnpm version:bump pre                      next pre-release of the same version (beta.1 → beta.2)
 *   pnpm version:bump pre --pre rc.1           same version with a later label
 *   pnpm version:bump release                  drop the label (rc.1 → final)
 *
 * It never commits, tags or publishes; it prints the tag to push for the release.
 */
import { execFileSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { msiConfigErrors } from "./tauriArgs.mjs";
import { VERSION_FILES, bumpVersion, msiVersion, readVersion, writeVersion } from "./versionBump.mjs";

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
    const version = current();
    const errors = msiConfigErrors(JSON.parse(read("app/src-tauri/tauri.conf.json")));
    if (errors.length) throw new Error(errors.join("\n"));
    console.log(`version ${version} (${VERSION_FILES.join(", ")}), MSI ${msiVersion(version)}`);
  } else if (command === "bump") {
    const preAt = rest.indexOf("--pre");
    const pre = preAt >= 0 ? rest[preAt + 1] : null;
    if (preAt >= 0 && !pre) throw new Error("--pre needs a label, e.g. --pre beta.1");
    const from = current();
    const to = bumpVersion(from, /** @type {any} */ (part), pre);
    for (const f of VERSION_FILES) writeFileSync(resolve(root, f), writeVersion(f, read(f), to));
    // Cargo.lock records the workspace crates' versions.
    execFileSync("cargo", ["update", "--workspace", "--offline"], { cwd: root, stdio: "inherit" });
    console.log(`${from} → ${to} (MSI ${msiVersion(to)})\nCommit the change, then push the tag v${to} to release it.`);
  } else {
    throw new Error("usage: version.mjs check | bump proud|default|shame|pre|release [--pre <label>]");
  }
} catch (e) {
  console.error(`error: ${e instanceof Error ? e.message : e}`);
  process.exit(1);
}
