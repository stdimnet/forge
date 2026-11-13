/**
 * Release helpers without I/O, used by release-tools.mjs and the release workflow:
 *
 * - releaseChannel: which update channel a release feeds (stable or beta) and
 *   whether its tag, version and pre-release flag agree.
 * - workflowTriggerErrors: the guard that the workflows' `on:` triggers never change
 *   (release only on pushed `v*` tags, everything else only on `workflow_dispatch`).
 * - tauriBuildErrors: the guard that workflows build through Forge's Tauri wrapper, which adds
 *   the signing settings and the MSI version (scripts/tauriArgs.mjs).
 * - parseChecksums / checksumFor: the release's `SHA256SUMS`.
 * - updatePkgbuild / srcinfo / updateFlatpakManifest / addMetainfoRelease: the AUR and
 *   Flatpak packaging files for a release.
 */

/** The `on:` block every workflow must have, by file name; any other file: `manual`. */
export const EXPECTED_TRIGGERS = {
  "release.yml": 'on:\n  push:\n    tags: ["v*"]',
};
/** The only trigger allowed for every other workflow. */
export const MANUAL_TRIGGER = "on:\n  workflow_dispatch:";

const SEMVER = /^(\d+)\.(\d+)\.(\d+)(?:-([0-9A-Za-z.-]+))?$/;

/**
 * @param {{ tag: string, prerelease: boolean, version: string }} release
 *   `version` is the app version of the tagged commit (tauri.conf.json).
 * @returns {{ channel: "stable" | "beta", errors: string[] }}
 */
export function releaseChannel({ tag, prerelease, version }) {
  const errors = [];
  const tagged = tag.replace(/^v/, "");
  const match = SEMVER.exec(tagged);
  if (!tag.startsWith("v") || !match) errors.push(`release tag ${tag} is not vX.Y.Z or vX.Y.Z-label`);
  if (tagged !== version) errors.push(`release tag ${tag} does not match the app version ${version}`);
  const labelled = !!match?.[4];
  if (labelled && !prerelease) errors.push(`${tag} has a pre-release label: publish it as a pre-release`);
  if (!labelled && prerelease) errors.push(`${tag} is marked as a pre-release but has no pre-release label (vX.Y.Z-beta.N)`);
  return { channel: labelled ? "beta" : "stable", errors };
}

/**
 * The top-level `on:` block of a workflow, without comments, trailing spaces and blank lines.
 * Returns every top-level `on` key (`on:`, `"on":`, `'on':`, `true:`) so a second one is found.
 * @param {string} text
 */
export function triggerBlocks(text) {
  const lines = text.split(/\r?\n/);
  const blocks = [];
  for (let i = 0; i < lines.length; i++) {
    if (!/^(on|"on"|'on'|true)\s*:/.test(lines[i])) continue;
    const block = [lines[i].replace(/\s+#.*$/, "").trimEnd()];
    for (let j = i + 1; j < lines.length; j++) {
      const line = lines[j];
      if (/^\S/.test(line)) break;
      const clean = line.replace(/(^|\s)#.*$/, "").trimEnd();
      if (clean) block.push(clean);
    }
    blocks.push(block.join("\n"));
  }
  return blocks;
}

/**
 * @param {{ name: string, text: string }[]} workflows the files in `.github/workflows`
 * @returns {string[]} one message per violation; empty when the triggers are as required
 */
export function workflowTriggerErrors(workflows) {
  const errors = [];
  for (const [name] of Object.entries(EXPECTED_TRIGGERS)) {
    if (!workflows.some((w) => w.name === name)) errors.push(`${name} is missing`);
  }
  for (const { name, text } of workflows) {
    const expected = EXPECTED_TRIGGERS[/** @type {keyof typeof EXPECTED_TRIGGERS} */ (name)] ?? MANUAL_TRIGGER;
    const blocks = triggerBlocks(text);
    if (blocks.length !== 1) {
      errors.push(`${name}: expected exactly one top-level \`on:\`, found ${blocks.length}`);
    } else if (blocks[0] !== expected) {
      errors.push(`${name}: triggers changed.\n  expected:\n${indent(expected)}\n  found:\n${indent(blocks[0])}`);
    }
  }
  return errors;
}

/** tauri-action's `tauriScript` that runs scripts/tauri.mjs (its `projectPath` is `app`). */
export const TAURI_SCRIPT = "pnpm --dir ui tauri";

/**
 * Workflows must build bundles through scripts/tauri.mjs: the plain Tauri CLI has no MSI
 * version for a pre-release (`0.1.1-beta.1`) and fails the Windows build.
 * @param {{ name: string, text: string }[]} workflows the files in `.github/workflows`
 * @returns {string[]} one message per violation
 */
export function tauriBuildErrors(workflows) {
  const errors = [];
  for (const { name, text } of workflows) {
    const lines = text.split(/\r?\n/).map((l) => l.replace(/(^|\s)#.*$/, "").trimEnd());
    lines.forEach((line, i) => {
      if (/\b(?:npx|cargo|pnpm exec|yarn|npm run)\s+tauri\s+(?:build|bundle)\b/.test(line)) {
        errors.push(`${name}:${i + 1}: runs the Tauri CLI directly; use \`pnpm tauri build\` (scripts/tauri.mjs)`);
      }
      const uses = /^(\s*)(-\s+)?uses:\s*tauri-apps\/tauri-action\b/.exec(line);
      if (!uses) return;
      // The step ends at the next line indented no deeper than its `- `.
      const depth = uses[1].length;
      let script = null;
      for (let j = i + 1; j < lines.length; j++) {
        const indentOf = /^\s*/.exec(lines[j])?.[0].length ?? 0;
        if (lines[j] && (indentOf < depth || (indentOf === depth && lines[j].trimStart().startsWith("-")))) break;
        const m = /^\s*tauriScript:\s*(.+)$/.exec(lines[j]);
        if (m) script = m[1].replace(/^["']|["']$/g, "");
      }
      if (script !== TAURI_SCRIPT) {
        errors.push(`${name}:${i + 1}: tauri-action needs \`tauriScript: ${TAURI_SCRIPT}\` (scripts/tauri.mjs)`);
      }
    });
  }
  return errors;
}

const indent = (/** @type {string} */ s) => s.replace(/^/gm, "    ");

/**
 * `SHA256SUMS` as `sha256sum` writes it: `<64 hex>  <name>` (or ` *<name>` in binary mode).
 * @param {string} text
 * @returns {Map<string, string>} file name → lower-case hex digest
 */
export function parseChecksums(text) {
  const sums = new Map();
  for (const line of text.split(/\r?\n/)) {
    const m = /^([0-9a-fA-F]{64}) [ *](.+)$/.exec(line.trim());
    if (m) sums.set(m[2].replace(/^.*\//, ""), m[1].toLowerCase());
  }
  return sums;
}

/** @param {Map<string, string>} sums @param {string} file */
export function checksumFor(sums, file) {
  const sum = sums.get(file);
  if (!sum) throw new Error(`SHA256SUMS has no entry for ${file}`);
  return sum;
}

/** The release's Debian package, which the AUR and Flatpak packages repack. */
export const debName = (/** @type {string} */ version) => `Forge_${version}_amd64.deb`;

/** A stable version (`X.Y.Z`) from a tag; packages follow the stable channel only. */
export function stableVersion(/** @type {string} */ tag) {
  const version = tag.replace(/^v/, "");
  const m = SEMVER.exec(version);
  if (!m) throw new Error(`${tag} is not a release tag (vX.Y.Z)`);
  if (m[4]) throw new Error(`${tag} is a pre-release: the AUR and Flatpak packages follow stable releases only`);
  return version;
}

/**
 * Set `pkgver`, reset `pkgrel` to 1 when the version changes, and replace the first
 * `sha256sums` entry (the .deb).
 * @param {string} text @param {{ version: string, sha256: string }} release
 */
export function updatePkgbuild(text, { version, sha256 }) {
  const current = /^pkgver=(.*)$/m.exec(text)?.[1];
  if (current === undefined) throw new Error("PKGBUILD has no pkgver");
  let out = text.replace(/^pkgver=.*$/m, `pkgver=${version}`);
  if (current !== version) out = out.replace(/^pkgrel=.*$/m, "pkgrel=1");
  if (!/^sha256sums=\('[^']*'/m.test(out)) throw new Error("PKGBUILD has no sha256sums=('…')");
  return out.replace(/^sha256sums=\('[^']*'/m, `sha256sums=('${sha256}'`);
}

/**
 * Read the simple assignments of a PKGBUILD: `key=value`, `key='value'`, `key=("a" 'b')`,
 * with `${pkgver}` / `$pkgname` expanded. Enough for Forge's own PKGBUILDs; makepkg remains
 * the reference (`makepkg --printsrcinfo`).
 * @param {string} text
 * @returns {Map<string, string[]>}
 */
export function pkgbuildFields(text) {
  const fields = new Map();
  /** @type {string[]} */
  const statements = [];
  let open = "";
  for (const line of text.split("\n")) {
    // Arrays may span lines: `optdepends=('a: x'\n            'b: y')`.
    if (open) {
      open += ` ${line.trim()}`;
      if (line.includes(")")) {
        statements.push(open);
        open = "";
      }
    } else if (/^[a-z0-9_]+=\(/.test(line) && !line.includes(")")) {
      open = line.trim();
    } else {
      statements.push(line);
    }
  }
  for (const line of statements) {
    const m = /^([a-z0-9_]+)=(.*)$/.exec(line.trim());
    if (!m) continue;
    const raw = m[2].replace(/\s+#.*$/, "");
    const values = raw.startsWith("(")
      ? [...raw.slice(1, raw.lastIndexOf(")")).matchAll(/'([^']*)'|"([^"]*)"|(\S+)/g)].map((v) => v[1] ?? v[2] ?? v[3])
      : [raw.replace(/^(['"])(.*)\1$/, "$2")];
    fields.set(m[1], values);
  }
  const expand = (/** @type {string} */ v) =>
    v.replace(/\$\{?([a-z_]+)\}?/g, (whole, name) => fields.get(name)?.[0] ?? whole);
  for (const [k, vs] of fields) fields.set(k, vs.map(expand));
  return fields;
}

/** The fields .SRCINFO lists for the package base, in makepkg's order. */
const SRCINFO_KEYS = [
  "pkgdesc", "pkgver", "pkgrel", "epoch", "url", "install", "changelog", "arch", "groups", "license",
  "checkdepends", "makedepends", "depends", "optdepends", "provides", "conflicts", "replaces",
  "backup", "options", "source", "validpgpkeys", "noextract", "md5sums", "sha1sums", "sha256sums",
  "b2sums",
];

/**
 * `.SRCINFO` for a single-package PKGBUILD, in the format of `makepkg --printsrcinfo`.
 * @param {string} text
 */
export function srcinfo(text) {
  const f = pkgbuildFields(text);
  const name = f.get("pkgname")?.[0];
  if (!name) throw new Error("PKGBUILD has no pkgname");
  const lines = [`pkgbase = ${name}`];
  for (const key of SRCINFO_KEYS) {
    for (const value of f.get(key) ?? []) if (value !== "") lines.push(`\t${key} = ${value}`);
  }
  lines.push("", `pkgname = ${name}`, "");
  return lines.join("\n");
}

/**
 * Point the Flatpak manifest's .deb source at a release: its `url:` and the `sha256:` right
 * after it.
 * @param {string} text @param {{ version: string, sha256: string }} release
 */
export function updateFlatpakManifest(text, { version, sha256 }) {
  const source = /(url: https:\/\/github\.com\/Reeperk\/Forge\/releases\/download\/)v[^/\s]+\/Forge_[^_\s]+_amd64\.deb(\s*\n\s*sha256: )[0-9a-f]{64}/;
  if (!source.test(text)) throw new Error("the Flatpak manifest has no release .deb source with url and sha256");
  return text.replace(source, `$1v${version}/${debName(version)}$2${sha256}`);
}

/**
 * Add `<release version="…" date="…"/>` as the newest entry of the AppStream metainfo,
 * unless that version is listed already.
 * @param {string} text @param {{ version: string, date: string }} release
 */
export function addMetainfoRelease(text, { version, date }) {
  if (text.includes(`<release version="${version}"`)) return text;
  const at = text.indexOf("<releases>");
  if (at < 0) throw new Error("the metainfo has no <releases>");
  const end = at + "<releases>".length;
  return `${text.slice(0, end)}\n    <release version="${version}" date="${date}"/>${text.slice(end)}`;
}

/**
 * The beta feed: the release's `latest.json` with `"channel": "beta"`, so stable
 * installations never take it even when someone points them at it.
 * @param {string} latestJson
 * @param {"stable" | "beta"} channel
 */
export function channelFeed(latestJson, channel) {
  const feed = JSON.parse(latestJson);
  if (typeof feed.version !== "string" || typeof feed.platforms !== "object") {
    throw new Error("latest.json has no version or platforms");
  }
  return `${JSON.stringify({ ...feed, channel }, null, 2)}\n`;
}
