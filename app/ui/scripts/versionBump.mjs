/**
 * Forge versions follow Pride Versioning (https://pridever.org): PROUD.DEFAULT.SHAME.
 *
 * - `proud`: a release we are really proud of; DEFAULT and SHAME go back to 0.
 * - `default`: a normal release; SHAME goes back to 0.
 * - `shame`: a fix too embarrassing to admit.
 * - `pre`: the next pre-release of the same version (`0.2.0-beta.1` → `0.2.0-beta.2`).
 * - `release`: the final version of a pre-release (`0.2.0-rc.1` → `0.2.0`).
 *
 * The result stays `X.Y.Z[-pre]`, so Cargo, npm and Tauri's updater read and order it like SemVer.
 */

const VERSION = /^(\d+)\.(\d+)\.(\d+)(?:-([0-9A-Za-z.-]+))?$/;
const PRE = /^[0-9A-Za-z]+(?:\.[0-9A-Za-z]+)*$/;

/** @param {string} version */
export function parseVersion(version) {
  const m = VERSION.exec(version.trim());
  if (!m) throw new Error(`"${version}" is not a PROUD.DEFAULT.SHAME version`);
  return { proud: Number(m[1]), default: Number(m[2]), shame: Number(m[3]), pre: m[4] ?? null };
}

/**
 * Compares two pre-release labels by SemVer precedence: numeric identifiers numerically,
 * others as text, and a label that is a prefix of the other comes first.
 * @param {string} a
 * @param {string} b
 */
export function comparePre(a, b) {
  const x = a.split(".");
  const y = b.split(".");
  for (let i = 0; i < Math.min(x.length, y.length); i++) {
    const xn = /^\d+$/.test(x[i]);
    const yn = /^\d+$/.test(y[i]);
    if (xn && yn) {
      const d = Number(x[i]) - Number(y[i]);
      if (d !== 0) return Math.sign(d);
    } else if (xn !== yn) {
      return xn ? -1 : 1;
    } else if (x[i] !== y[i]) {
      return x[i] < y[i] ? -1 : 1;
    }
  }
  return Math.sign(x.length - y.length);
}

/** The label after `pre`: its last number plus one (`beta.1` → `beta.2`), else `.1` appended. @param {string} pre */
function nextPre(pre) {
  const parts = pre.split(".");
  const last = parts[parts.length - 1];
  if (/^\d+$/.test(last)) parts[parts.length - 1] = String(Number(last) + 1);
  else parts.push("1");
  return parts.join(".");
}

/**
 * The next version after `version`.
 * @param {string} version
 * @param {"proud" | "default" | "shame" | "pre" | "release"} part
 * @param {string | null} [pre] pre-release label, e.g. `beta.1`
 */
export function bumpVersion(version, part, pre = null) {
  const v = parseVersion(version);
  if (pre !== null && !PRE.test(pre)) throw new Error(`"${pre}" is not a valid pre-release label (e.g. beta.1)`);
  if (part === "pre") {
    if (v.pre === null) throw new Error(`${version} is not a pre-release; use proud, default or shame with --pre`);
    const label = pre ?? nextPre(v.pre);
    if (comparePre(label, v.pre) <= 0) throw new Error(`"${label}" does not come after "${v.pre}"`);
    return `${v.proud}.${v.default}.${v.shame}-${label}`;
  }
  if (part === "release") {
    if (v.pre === null) throw new Error(`${version} is already a release`);
    if (pre !== null) throw new Error("release takes no --pre label");
    return `${v.proud}.${v.default}.${v.shame}`;
  }
  let next;
  switch (part) {
    case "proud":
      next = [v.proud + 1, 0, 0];
      break;
    case "default":
      next = [v.proud, v.default + 1, 0];
      break;
    case "shame":
      next = [v.proud, v.default, v.shame + 1];
      break;
    default:
      throw new Error(`unknown part "${part}": use proud, default, shame, pre or release`);
  }
  return next.join(".") + (pre ? `-${pre}` : "");
}

/** Files that carry the version, relative to the repository root. */
export const VERSION_FILES = ["Cargo.toml", "app/src-tauri/tauri.conf.json", "app/ui/package.json"];

/** The version written in one of `VERSION_FILES`, or null. @param {string} file @param {string} text */
export function readVersion(file, text) {
  if (file.endsWith("Cargo.toml")) {
    const section = /\[workspace\.package\][^[]*?\nversion\s*=\s*"([^"]+)"/.exec(text);
    return section ? section[1] : null;
  }
  return JSON.parse(text).version ?? null;
}

/** `text` with the version replaced. @param {string} file @param {string} text @param {string} version */
export function writeVersion(file, text, version) {
  if (file.endsWith("Cargo.toml")) {
    const out = text.replace(/(\[workspace\.package\][^[]*?\nversion\s*=\s*")[^"]+(")/, `$1${version}$2`);
    if (out === text && readVersion(file, text) !== version) throw new Error(`no [workspace.package] version in ${file}`);
    return out;
  }
  // Only the top-level "version" line, so the file's own formatting stays as it is.
  const out = text.replace(/^(\s{2}"version":\s*")[^"]+(")/m, `$1${version}$2`);
  if (readVersion(file, out) !== version) throw new Error(`no top-level "version" in ${file}`);
  return out;
}
