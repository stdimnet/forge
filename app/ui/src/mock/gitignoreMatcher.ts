/**
 * `.gitignore` matching for the browser-mode demo project. Mirrors the Rust implementation
 * (`crates/forge-core/src/gitignore.rs`): `.gitignore` files from the root down (the deepest
 * match wins), negation, directory-only and anchored patterns, `**`, and everything inside an
 * ignored directory is ignored.
 */

interface Rule {
  re: RegExp;
  negate: boolean;
  dirOnly: boolean;
}

const escapeRe = (c: string) => c.replace(/[\\^$.*+?()[\]{}|/]/g, "\\$&");

function globToRegExp(glob: string, anchored: boolean): RegExp {
  let out = "";
  for (let i = 0; i < glob.length; i++) {
    const c = glob[i];
    if (c === "*") {
      if (glob[i + 1] === "*") {
        const atStart = i === 0 || glob[i - 1] === "/";
        if (atStart && glob[i + 2] === "/") {
          out += "(?:.*/)?";
          i += 2;
        } else if (atStart && i + 2 === glob.length) {
          out += ".*";
          i += 1;
        } else {
          out += "[^/]*";
          i += 1;
        }
      } else out += "[^/]*";
    } else if (c === "?") out += "[^/]";
    else if (c === "[") {
      const end = glob.indexOf("]", i + 2);
      if (end < 0) out += "\\[";
      else {
        let cls = glob.slice(i + 1, end);
        if (cls[0] === "!") cls = `^${cls.slice(1)}`;
        out += `[${cls.replace(/\\/g, "\\\\")}]`;
        i = end;
      }
    } else if (c === "\\" && i + 1 < glob.length) out += escapeRe(glob[++i]);
    else out += escapeRe(c);
  }
  return new RegExp(`^${anchored ? "" : "(?:.*/)?"}${out}$`);
}

export function parseGitignore(text: string): Rule[] {
  const rules: Rule[] = [];
  for (const raw of text.split("\n")) {
    let line = raw.replace(/\r$/, "").replace(/(?<!\\)\s+$/, "");
    if (!line || line.startsWith("#")) continue;
    const negate = line.startsWith("!");
    if (negate) line = line.slice(1);
    if (line.startsWith("\\#") || line.startsWith("\\!")) line = line.slice(1);
    const dirOnly = line.endsWith("/");
    if (dirOnly) line = line.slice(0, -1);
    if (!line) continue;
    const anchored = line.includes("/");
    if (line.startsWith("/")) line = line.slice(1);
    rules.push({ re: globToRegExp(line, anchored), negate, dirOnly });
  }
  return rules;
}

/** `read(dir)` returns the `.gitignore` text of a project-relative directory ("" is the root). */
export function createIgnoreMatcher(read: (dir: string) => string | undefined) {
  const cache = new Map<string, Rule[]>();
  const rulesOf = (dir: string) => {
    let rules = cache.get(dir);
    if (!rules) {
      rules = parseGitignore(read(dir) ?? "");
      cache.set(dir, rules);
    }
    return rules;
  };

  /** Decision of the rules visible from `dir` for `path`: true/false, or undefined for no match. */
  const decide = (parts: string[], isDir: boolean): boolean | undefined => {
    for (let depth = parts.length - 1; depth >= 0; depth--) {
      const dir = parts.slice(0, depth).join("/");
      const rel = parts.slice(depth).join("/");
      const rules = rulesOf(dir);
      for (let i = rules.length - 1; i >= 0; i--) {
        const r = rules[i];
        if ((r.dirOnly && !isDir) || !r.re.test(rel)) continue;
        return !r.negate;
      }
    }
    return undefined;
  };

  return {
    isIgnored(path: string, isDir: boolean): boolean {
      const parts = path.split("/").filter(Boolean);
      for (let n = 1; n <= parts.length; n++) {
        const last = n === parts.length;
        if (decide(parts.slice(0, n), last ? isDir : true) === true) return true;
      }
      return false;
    },
  };
}
