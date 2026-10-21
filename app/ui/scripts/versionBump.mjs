/**
 * Forge versions follow Pride Versioning (https://pridever.org): PROUD.DEFAULT.SHAME.
 *
 * - `proud`: a release we are really proud of; DEFAULT and SHAME go back to 0.
 * - `default`: a normal release; SHAME goes back to 0.
 * - `shame`: a fix too embarrassing to admit.
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
 * The next version after `version`.
 * @param {string} version
 * @param {"proud" | "default" | "shame"} part
 * @param {string | null} [pre] pre-release label, e.g. `beta.1`
 */
export function bumpVersion(version, part, pre = null) {
  const v = parseVersion(version);
  if (pre !== null && !PRE.test(pre)) throw new Error(`"${pre}" is not a valid pre-release label (e.g. beta.1)`);
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
      throw new Error(`unknown part "${part}": use proud, default or shame`);
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
