/** Helpers for PHP servers and path mappings in the UI. */
import type { PathMapping, ServerConfig, SessionInfo } from "./api";

/** `file:///var/www/a%20b.php` → `/var/www/a b.php`. */
export function uriToPath(uri: string): string {
  const raw = uri.replace(/^file:\/\//, "");
  let path: string;
  try {
    path = decodeURIComponent(raw);
  } catch {
    path = raw;
  }
  return /^\/[A-Za-z]:\//.test(path) ? path.slice(1) : path;
}

/**
 * The directory mapping implied by "server file X is project file Y": the longest common
 * trailing path is cut from both. `/var/www/html/public/index.php` + `public/index.php`
 * gives `{ remote: "/var/www/html", local: "" }`.
 */
export function suggestMapping(remoteFile: string, localFile: string): PathMapping {
  const r = remoteFile.split("/").filter((p, i) => p || i === 0);
  const l = localFile.split("/").filter(Boolean);
  let n = 0;
  while (n < l.length && n < r.length - 1 && r[r.length - 1 - n] === l[l.length - 1 - n]) n++;
  if (n === 0) return { remote: remoteFile, local: localFile };
  const remote = r.slice(0, r.length - n).join("/") || "/";
  return { remote, local: l.slice(0, l.length - n).join("/") };
}

export function slug(s: string): string {
  return s.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "") || "server";
}

/** A new server for an incoming connection, mapped with `mapping`. */
export function serverFor(session: SessionInfo, mapping: PathMapping | null, existing: ServerConfig[]): ServerConfig {
  const name = session.serverName === "CLI" ? session.host ?? "localhost" : session.serverName.replace(/:\d+$/, "");
  let id = slug(name);
  for (let i = 2; existing.some((s) => s.id === id); i++) id = `${slug(name)}-${i}`;
  return {
    id,
    name,
    host: session.host ?? name,
    port: session.port ?? 80,
    debugger: "Xdebug",
    usePathMappings: !!mapping,
    mappings: mapping ? [mapping] : [],
  };
}
