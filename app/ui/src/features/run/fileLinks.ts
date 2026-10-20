/**
 * Finds file references in console and terminal output (`app/User.php:42`,
 * `/abs/path/File.php on line 7`, `File.php(12)`, `#0 /path/x.php(31): …`) so they can be
 * clicked to open the file at that line.
 */

export interface FileLink {
  /** Offsets in the text (end exclusive). */
  start: number;
  end: number;
  path: string;
  /** 1-based. */
  line?: number;
  column?: number;
}

const EXT = "[A-Za-z][A-Za-z0-9]{0,6}";
const PATH = `(?:[A-Za-z]:[\\\\/]|~?/|\\.{1,2}/)?[\\w.@~-]+(?:[\\\\/][\\w.@~-]+)*\\.${EXT}`;
const RE = new RegExp(`(${PATH})(?::(\\d+)(?::(\\d+))?| on line (\\d+)|\\((\\d+)\\))?`, "g");

export function findFileLinks(text: string): FileLink[] {
  const out: FileLink[] = [];
  RE.lastIndex = 0;
  let m: RegExpExecArray | null;
  while ((m = RE.exec(text))) {
    const path = m[1];
    const before = text[m.index - 1];
    const line = m[2] ?? m[4] ?? m[5];
    // Skip URLs, e-mail addresses and bare names like `v1.2` or `README.md` without a line.
    if (before === ":" || before === "/" || before === "@" || /^\d/.test(path)) continue;
    if (/^\w+:\/\//.test(text.slice(Math.max(0, m.index - 10), m.index + 3))) continue;
    if (!line && !/[\\/]/.test(path)) continue;
    if (/^[\d.]+$/.test(path.replace(/\.[A-Za-z]+$/, ""))) continue;
    out.push({
      start: m.index,
      end: m.index + m[0].length,
      path,
      line: line ? Number(line) : undefined,
      column: m[3] ? Number(m[3]) : undefined,
    });
  }
  return out;
}

/**
 * Map a path from output to a workspace-relative path: strips the project root, resolves
 * `./` and keeps relative paths (relative to `cwd`, which is the root by default).
 * Returns `null` for absolute paths outside the project.
 */
export function toWorkspacePath(path: string, root: string | undefined, cwdRelative = ""): string | null {
  let p = path.replace(/\\/g, "/");
  const r = root?.replace(/\\/g, "/").replace(/\/+$/, "");
  if (r && (p === r || p.startsWith(`${r}/`))) return p.slice(r.length + 1);
  if (/^([A-Za-z]:\/|\/|~\/)/.test(p)) return null;
  p = p.replace(/^\.\//, "");
  const base = cwdRelative.replace(/^\/+|\/+$/g, "");
  const parts = (base ? `${base}/${p}` : p).split("/");
  const out: string[] = [];
  for (const part of parts) {
    if (part === "..") {
      if (!out.length) return null;
      out.pop();
    } else if (part !== ".") out.push(part);
  }
  return out.join("/");
}
