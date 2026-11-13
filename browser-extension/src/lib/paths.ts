/**
 * Map the source references a page knows (`webpack:///./src/App.tsx`, `/@fs/home/me/…`,
 * `http://localhost:5173/src/App.tsx?t=1`, Docker paths, …) to files Forge can open.
 */
import type { OpenTarget, PathMapping } from "./types";

export interface PathContext {
  /** Roots of the projects open in Forge (from `/api/forge/ping`). */
  roots: string[];
  /** User-defined prefix rewrites, e.g. `/var/www/html` → `/home/me/shop`. */
  mappings?: PathMapping[];
}

/** Absolute paths that start like this are taken as real file system paths. */
const SYSTEM_PREFIXES = [
  "/home/",
  "/Users/",
  "/root/",
  "/var/",
  "/srv/",
  "/opt/",
  "/tmp/",
  "/mnt/",
  "/media/",
  "/private/",
  "/usr/",
  "/workspace/",
  "/workspaces/",
  "/app/",
  "/code/",
];

const isWindowsAbsolute = (p: string) => /^[A-Za-z]:\//.test(p);
const isAbsolute = (p: string) => p.startsWith("/") || isWindowsAbsolute(p);
const trimSlash = (p: string) => (p.length > 1 ? p.replace(/\/+$/, "") : p);

function safeDecode(s: string): string {
  try {
    return decodeURIComponent(s);
  } catch {
    return s;
  }
}

/** Remove `.`/`..` segments (never above the start of the path). */
export function normalizeSegments(path: string): string {
  const abs = path.startsWith("/");
  const out: string[] = [];
  for (const seg of path.split("/")) {
    if (seg === "" || seg === ".") continue;
    if (seg === ".." && out.length && out[out.length - 1] !== "..") out.pop();
    else if (seg !== ".." || !abs) out.push(seg);
  }
  return (abs ? "/" : "") + out.join("/");
}

/**
 * Turn any source reference into a plain path: strips URL schemes, dev-server prefixes,
 * query strings and hashes. Relative results have no leading `./`.
 */
export function cleanSourcePath(raw: string): string {
  let p = raw.trim().replace(/\\/g, "/");
  if (p.startsWith("file://")) {
    p = safeDecode(p.slice("file://".length).replace(/^localhost(?=\/)/, ""));
    if (/^\/[A-Za-z]:\//.test(p)) p = p.slice(1);
    return normalizeSegments(p);
  }
  // webpack://<namespace>/./src/App.tsx, webpack:///src/App.tsx, webpack:////abs/path
  const webpack = /^webpack(?:-internal)?:\/\/[^/]*\/(.*)$/.exec(p);
  if (webpack) p = webpack[1];
  else {
    const url = /^[a-z][a-z0-9+.-]*:\/\/[^/]*(\/[^?#]*)?/i.exec(p);
    if (url) p = safeDecode(url[1] ?? "/");
  }
  p = p.replace(/[?#].*$/, "");
  // Vite: /@fs/abs/path (outside the root), /@id/… for virtual modules.
  if (p.startsWith("/@fs/")) {
    p = p.slice("/@fs".length);
    if (/^\/[A-Za-z]:\//.test(p)) p = p.slice(1);
  }
  if (isWindowsAbsolute(p)) return p.slice(0, 2) + normalizeSegments(p.slice(2));
  return normalizeSegments(p.replace(/^\.\//, ""));
}

function applyMappings(path: string, mappings: PathMapping[] = []): string {
  for (const m of mappings) {
    const from = trimSlash(m.from.replace(/\\/g, "/"));
    if (from && (path === from || path.startsWith(`${from}/`))) {
      return trimSlash(m.to.replace(/\\/g, "/")) + path.slice(from.length);
    }
  }
  return path;
}

const under = (path: string, root: string) => {
  const r = trimSlash(root.replace(/\\/g, "/"));
  return path === r || path.startsWith(`${r}/`);
};

/**
 * Resolve a raw reference to what `POST /api/forge/open` needs. Root-relative dev-server
 * paths (`/src/App.tsx`) and relative ones are joined to the first project root.
 */
export function toOpenTarget(
  raw: string,
  ctx: PathContext,
  position: { line?: number; column?: number; symbol?: string } = {},
): OpenTarget {
  const path = applyMappings(cleanSourcePath(raw), ctx.mappings);
  const roots = ctx.roots.map((r) => trimSlash(r.replace(/\\/g, "/")));
  const base = { line: position.line, column: position.column, symbol: position.symbol };
  if (isAbsolute(path)) {
    const known = roots.some((r) => under(path, r));
    const system = isWindowsAbsolute(path) || SYSTEM_PREFIXES.some((s) => path.startsWith(s));
    if (known || system || !roots.length) return { ...base, file: path };
    // `/src/App.tsx` as served by a dev server: relative to the project.
    return { ...base, file: `${roots[0]}${path}`, project: roots[0] };
  }
  if (roots.length) return { ...base, file: normalizeSegments(`${roots[0]}/${path}`), project: roots[0] };
  return { ...base, file: path };
}

/** Parse `from => to` lines (blank lines and lines without `=>` are ignored). */
export function parseMappings(text: string): PathMapping[] {
  return text
    .split("\n")
    .map((line) => line.split("=>").map((s) => s.trim()))
    .filter((parts) => parts.length === 2 && parts[0] && parts[1])
    .map(([from, to]) => ({ from, to }));
}

export const formatMappings = (mappings: PathMapping[]) => mappings.map((m) => `${m.from} => ${m.to}`).join("\n");

/** Short label for a file: relative to its project root, else the last segments. */
export function displayPath(file: string, roots: string[]): string {
  const f = file.replace(/\\/g, "/");
  for (const r of roots.map((r) => trimSlash(r.replace(/\\/g, "/")))) {
    if (f.startsWith(`${r}/`)) return f.slice(r.length + 1);
  }
  const parts = cleanSourcePath(f).split("/").filter(Boolean);
  return parts.slice(-3).join("/");
}
