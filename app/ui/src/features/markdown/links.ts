/**
 * Links and image sources of the Markdown preview: what a click does and where a relative
 * path points inside the project.
 */

export type LinkTarget =
  /** `#anchor` in the same document. */
  | { kind: "anchor"; anchor: string }
  /** `https:`, `mailto:` and other schemes: opened by the system browser / mail client. */
  | { kind: "external"; url: string }
  /** A project file, with an optional `#anchor` (a heading, or `L12` for a line). */
  | { kind: "file"; path: string; anchor?: string }
  /** Nothing to open (empty, `javascript:`, outside the project). */
  | { kind: "none" };

const SAFE_SCHEMES = /^(https?|mailto|ftp|tel):/i;

/** Normalize `a/./b/../c` into `a/c`; null when it climbs out of the project. */
export function normalizePath(path: string): string | null {
  const out: string[] = [];
  for (const part of path.split("/")) {
    if (!part || part === ".") continue;
    if (part === "..") {
      if (!out.length) return null;
      out.pop();
    } else out.push(part);
  }
  return out.join("/");
}

/** A path written in `doc` resolved to a workspace-relative path (`/x` is the project root). */
export function resolveRelative(doc: string, target: string): string | null {
  let t = target;
  try {
    t = decodeURI(target);
  } catch {
    /* keep it as written */
  }
  if (t.startsWith("/")) return normalizePath(t.slice(1));
  const dir = doc.includes("/") ? doc.slice(0, doc.lastIndexOf("/")) : "";
  return normalizePath(dir ? `${dir}/${t}` : t);
}

/** What a link of the document at `doc` opens. */
export function linkTarget(doc: string, href: string): LinkTarget {
  const h = href.trim();
  if (!h) return { kind: "none" };
  if (h.startsWith("#")) return { kind: "anchor", anchor: h.slice(1) };
  if (/^[a-z][a-z0-9+.-]*:/i.test(h)) return SAFE_SCHEMES.test(h) ? { kind: "external", url: h } : { kind: "none" };
  if (h.startsWith("//")) return { kind: "external", url: `https:${h}` };
  const hash = h.indexOf("#");
  const pathPart = (hash >= 0 ? h.slice(0, hash) : h).split("?")[0];
  const anchor = hash >= 0 ? h.slice(hash + 1) : undefined;
  if (!pathPart || pathPart === "." || pathPart === "./") return anchor ? { kind: "anchor", anchor } : { kind: "none" };
  const path = resolveRelative(doc, pathPart);
  if (!path) return { kind: "none" };
  return { kind: "file", path, anchor: anchor || undefined };
}

/** `L12` / `L12-L20` (GitHub line anchors) → 1-based line. */
export function lineAnchor(anchor: string | undefined): number | undefined {
  const m = anchor?.match(/^L(\d+)/);
  return m ? Number(m[1]) : undefined;
}

export type ImageSource = { kind: "url"; url: string } | { kind: "file"; path: string } | { kind: "none" };

/** Where an image of the document at `doc` comes from. */
export function imageSource(doc: string, src: string): ImageSource {
  const s = src.trim();
  if (/^https?:\/\//i.test(s)) return { kind: "url", url: s };
  if (/^data:image\/(png|jpe?g|gif|webp|bmp|x-icon|svg\+xml)[;,]/i.test(s)) return { kind: "url", url: s };
  if (/^[a-z][a-z0-9+.-]*:/i.test(s) || s.startsWith("//")) return { kind: "none" };
  const path = resolveRelative(doc, s.split(/[?#]/)[0]);
  return path ? { kind: "file", path } : { kind: "none" };
}
