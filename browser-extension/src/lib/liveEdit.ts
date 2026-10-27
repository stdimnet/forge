/**
 * Live Edit in the page: Forge reports changed files (`GET /api/forge/live-edit`); style
 * sheets the page loaded are swapped in place (with the editor's unsaved text, or by
 * reloading the `<link>` with a cache-busting query), other files reload the page when it
 * loaded them. Pure DOM code, so it is unit tested with jsdom.
 */

export interface LiveEditEvent {
  seq: number;
  kind: "css" | "reload";
  /** Project-relative path. */
  file: string;
  /** File name. */
  name: string;
  text?: string;
}

export interface LivePoll {
  enabled: boolean;
  seq: number;
  events: LiveEditEvent[];
}

const STYLE_ATTR = "data-forge-live";

/** The path part of a URL (without query and hash), or "" for unparsable ones. */
function pathOf(url: string, base: string): string {
  try {
    return new URL(url, base).pathname;
  } catch {
    return "";
  }
}

/** Folders a web server serves from, which never appear in URLs. */
const WEB_ROOTS = ["public/", "static/", "web/", "www/", "htdocs/", "wwwroot/", "dist/"];

/**
 * Whether the URL path of a resource is the changed (project-relative) file: the URL path
 * ends with the file's path below its web root (`/css/app.css` is `public/css/app.css`,
 * `/src/App.vue` is `src/App.vue` on a Vite dev server).
 */
export function sameFile(urlPath: string, file: string, name: string): boolean {
  const p = decodeURIComponent(urlPath).replace(/\/+$/, "");
  if (!p.endsWith(`/${name}`) && p !== name) return false;
  let rel = file.replace(/^\.?\//, "");
  for (const root of WEB_ROOTS) {
    const i = rel.indexOf(root);
    if (i === 0 || (i > 0 && rel[i - 1] === "/")) {
      rel = rel.slice(i + root.length);
      break;
    }
  }
  return p === `/${rel}` || p.endsWith(`/${rel}`);
}

/** The URL paths of everything the page loaded: itself, style sheets, scripts, resources. */
export function loadedPaths(doc: Document, resources: string[] = []): string[] {
  const base = doc.location?.href ?? "http://localhost/";
  const urls = [
    base,
    ...Array.from(doc.querySelectorAll<HTMLLinkElement>("link[href]")).map((l) => l.href || l.getAttribute("href") || ""),
    ...Array.from(doc.querySelectorAll<HTMLScriptElement>("script[src]")).map((s) => s.src || s.getAttribute("src") || ""),
    ...resources,
  ];
  return urls.map((u) => pathOf(u, base)).filter(Boolean);
}

/** Whether the page loaded the changed file. */
export function pageUses(doc: Document, e: LiveEditEvent, resources: string[] = []): boolean {
  if (e.name.endsWith(".html") || e.name.endsWith(".htm")) {
    const here = pathOf(doc.location?.href ?? "", "http://localhost/");
    if (here.endsWith("/") && e.name === "index.html") return true;
  }
  return loadedPaths(doc, resources).some((p) => sameFile(p, e.file, e.name));
}

/**
 * Swap the matching style sheets. With `text` the sheet is replaced by an inline `<style>`
 * (the `<link>` is disabled); without, the `<link>` is reloaded with `?forge-live=<seq>`.
 * Returns how many sheets changed.
 */
export function applyCss(doc: Document, e: LiveEditEvent): number {
  const base = doc.location?.href ?? "http://localhost/";
  let changed = 0;
  for (const link of Array.from(doc.querySelectorAll<HTMLLinkElement>('link[rel~="stylesheet"][href]'))) {
    const href = link.getAttribute("href") ?? "";
    if (!sameFile(pathOf(href, base), e.file, e.name)) continue;
    changed++;
    if (e.text !== undefined) {
      let style = link.nextElementSibling as HTMLStyleElement | null;
      if (!(style instanceof HTMLStyleElement) || style.getAttribute(STYLE_ATTR) !== e.name) {
        style = doc.createElement("style");
        style.setAttribute(STYLE_ATTR, e.name);
        link.after(style);
      }
      style.textContent = e.text;
      link.disabled = true;
    } else {
      // Saved: back to the file itself, freshly loaded.
      const next = link.nextElementSibling;
      if (next instanceof HTMLStyleElement && next.getAttribute(STYLE_ATTR) === e.name) next.remove();
      const url = new URL(href, base);
      url.searchParams.set("forge-live", String(e.seq));
      link.href = url.toString();
      link.disabled = false;
    }
  }
  return changed;
}

/** What the page does for an event: `css` (swapped), `reload`, or nothing. */
export function plan(doc: Document, e: LiveEditEvent, resources: string[] = []): "css" | "reload" | null {
  if (e.kind === "css") {
    if (doc.querySelector('link[rel~="stylesheet"][href]') && Array.from(doc.querySelectorAll<HTMLLinkElement>('link[rel~="stylesheet"][href]')).some((l) => sameFile(pathOf(l.getAttribute("href") ?? "", doc.location?.href ?? "http://localhost/"), e.file, e.name))) {
      return "css";
    }
    // A style sheet the page imports some other way (bundlers): reload when it was loaded.
    return pageUses(doc, e, resources) ? "reload" : null;
  }
  return pageUses(doc, e, resources) ? "reload" : null;
}
