/**
 * Release helpers (logic and tests in releaseTools.mjs). Nothing here publishes anything.
 *
 *   node scripts/release-tools.mjs workflows          check the workflows' `on:` triggers
 *   node scripts/release-tools.mjs channel            release workflow: stable or beta
 *       (reads RELEASE_TAG and RELEASE_PRERELEASE, writes `channel=…` to GITHUB_OUTPUT)
 *   node scripts/release-tools.mjs feed <channel> <latest.json> <out>
 *   node scripts/release-tools.mjs packaging <vX.Y.Z> [--sums SHA256SUMS] [--date YYYY-MM-DD]
 *       point the AUR PKGBUILD and the Flatpak manifest at a release and regenerate .SRCINFO
 *   node scripts/release-tools.mjs srcinfo            regenerate the .SRCINFO files
 */
import { appendFileSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import {
  addMetainfoRelease,
  channelFeed,
  checksumFor,
  debName,
  parseChecksums,
  releaseChannel,
  srcinfo,
  stableVersion,
  updateFlatpakManifest,
  updatePkgbuild,
  workflowTriggerErrors,
} from "./releaseTools.mjs";

const root = resolve(import.meta.dirname, "../../..");
const at = (/** @type {string} */ f) => resolve(root, f);
const read = (/** @type {string} */ f) => readFileSync(at(f), "utf8");

const PKGBUILDS = ["packaging/aur/forge-bin", "packaging/aur/forge-git"];
const MANIFEST = "packaging/flatpak/dev.forge.ide.yml";
const METAINFO = "packaging/flatpak/dev.forge.ide.metainfo.xml";

function writeSrcinfo() {
  for (const dir of PKGBUILDS) {
    writeFileSync(at(`${dir}/.SRCINFO`), srcinfo(read(`${dir}/PKGBUILD`)));
    console.log(`wrote ${dir}/.SRCINFO`);
  }
}

/** @param {string[]} args @param {string} name */
const option = (args, name) => {
  const i = args.indexOf(name);
  return i >= 0 ? args[i + 1] : undefined;
};

async function packaging(/** @type {string[]} */ args) {
  const tag = args[0];
  if (!tag) throw new Error("usage: packaging vX.Y.Z [--sums SHA256SUMS] [--date YYYY-MM-DD]");
  const version = stableVersion(tag);
  const sumsFile = option(args, "--sums");
  let text;
  if (sumsFile) {
    text = readFileSync(resolve(sumsFile), "utf8");
  } else {
    const url = `https://github.com/Reeperk/Forge/releases/download/${tag}/SHA256SUMS`;
    console.log(`fetching ${url}`);
    const response = await fetch(url);
    if (!response.ok) throw new Error(`${url}: HTTP ${response.status}`);
    text = await response.text();
  }
  const sha256 = checksumFor(parseChecksums(text), debName(version));
  const date = option(args, "--date") ?? new Date().toISOString().slice(0, 10);
  const pkgbuild = "packaging/aur/forge-bin/PKGBUILD";
  writeFileSync(at(pkgbuild), updatePkgbuild(read(pkgbuild), { version, sha256 }));
  writeFileSync(at(MANIFEST), updateFlatpakManifest(read(MANIFEST), { version, sha256 }));
  writeFileSync(at(METAINFO), addMetainfoRelease(read(METAINFO), { version, date }));
  console.log(`${debName(version)}: ${sha256}`);
  writeSrcinfo();
}

const [command, ...args] = process.argv.slice(2);
try {
  if (command === "workflows") {
    const dir = ".github/workflows";
    const workflows = readdirSync(at(dir))
      .filter((f) => /\.ya?ml$/.test(f))
      .map((name) => ({ name, text: read(`${dir}/${name}`) }));
    const errors = workflowTriggerErrors(workflows);
    if (errors.length) {
      for (const e of errors) console.error(e);
      throw new Error("workflow triggers must stay: release.yml on pushed `v*` tags, all others on `workflow_dispatch` only");
    }
    console.log(`workflow triggers unchanged (${workflows.map((w) => w.name).join(", ")})`);
  } else if (command === "channel") {
    const tag = process.env.RELEASE_TAG ?? "";
    const prerelease = process.env.RELEASE_PRERELEASE === "true";
    const version = JSON.parse(read("app/src-tauri/tauri.conf.json")).version;
    const { channel, errors } = releaseChannel({ tag, prerelease, version });
    if (errors.length) throw new Error(errors.join("\n"));
    console.log(`${tag}: ${channel} channel`);
    if (process.env.GITHUB_OUTPUT) appendFileSync(process.env.GITHUB_OUTPUT, `channel=${channel}\n`);
  } else if (command === "feed") {
    const [channel, input, output] = args;
    if ((channel !== "stable" && channel !== "beta") || !input || !output) {
      throw new Error("usage: feed stable|beta <latest.json> <out>");
    }
    writeFileSync(resolve(output), channelFeed(readFileSync(resolve(input), "utf8"), channel));
  } else if (command === "packaging") {
    await packaging(args);
  } else if (command === "srcinfo") {
    writeSrcinfo();
  } else {
    throw new Error("usage: release-tools.mjs workflows | channel | feed | packaging <tag> | srcinfo");
  }
} catch (e) {
  console.error(`error: ${e instanceof Error ? e.message : e}`);
  process.exit(1);
}
