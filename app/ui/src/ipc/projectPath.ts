/**
 * Project paths of Code With Me guest tabs. A guest shows the host's shared files as
 * `cwm://<project-relative path>` tabs so they never mix with other tabs; the backend knows
 * only the project path. Every IPC call maps such tab paths to project paths (`invoke`),
 * so features can pass a tab's path as they do for any other file.
 */

/** Prefix of a guest's tabs for the host's shared files (`cwm://app/Models/Order.php`). */
export const GUEST_PREFIX = "cwm://";

/** A guest's project root as the UI shows it (`cwm://<host>/<name>`): not a tab path. */
let guestRoot: string | null = null;

/** Set by the workspace state when a project opens or closes. */
export function setGuestRoot(root: string | null): void {
  guestRoot = root?.startsWith(GUEST_PREFIX) ? root : null;
}

/** The project path of a tab: `cwm://app/x.php` is `app/x.php`; other paths stay as they are. */
export function projectPathOf(path: string): string {
  if (!path.startsWith(GUEST_PREFIX)) return path;
  if (guestRoot !== null && (path === guestRoot || path.startsWith(`${guestRoot}/`))) return path;
  return path.slice(GUEST_PREFIX.length);
}

/** A `cwm://` tab path (but not the guest's project root). */
export const isGuestTabPath = (path: string): boolean => projectPathOf(path) !== path;

/** Argument names that hold file paths: `path`, `paths`, `file`, `files`, `filePath`, `newPath`, … */
const PATH_KEY = /(?:path|paths|file|files)$/i;
const MAX_DEPTH = 4;

function mapValue(value: unknown, pathKey: boolean, depth: number): unknown {
  if (typeof value === "string") return pathKey ? projectPathOf(value) : value;
  if (depth >= MAX_DEPTH || value === null || typeof value !== "object") return value;
  if (Array.isArray(value)) {
    let out: unknown[] | null = null;
    for (let i = 0; i < value.length; i++) {
      const mapped = mapValue(value[i], pathKey, depth + 1);
      if (mapped !== value[i]) (out ??= value.slice())[i] = mapped;
    }
    return out ?? value;
  }
  // Plain objects only (not ArrayBuffers, typed arrays or class instances).
  const proto = Object.getPrototypeOf(value);
  if (proto !== Object.prototype && proto !== null) return value;
  return mapObject(value as Record<string, unknown>, depth + 1);
}

function mapObject(obj: Record<string, unknown>, depth: number): Record<string, unknown> {
  let out: Record<string, unknown> | null = null;
  for (const key of Object.keys(obj)) {
    const value = obj[key];
    const mapped = mapValue(value, PATH_KEY.test(key), depth);
    if (mapped !== value) (out ??= { ...obj })[key] = mapped;
  }
  return out ?? obj;
}

/**
 * IPC arguments with guest tab paths mapped to project paths, in arguments named like paths
 * (also nested ones, e.g. `query.paths`). Unchanged arguments are returned as they are.
 */
export const withProjectPaths = (args: Record<string, unknown>): Record<string, unknown> => mapObject(args, 0);
