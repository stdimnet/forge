/** Match a file path against a `;`-separated list of simple globs (`*`, `?`), e.g. "*.md; *.txt". */
export function matchesGlobList(path: string, patterns: string): boolean {
  const name = path.split("/").pop() ?? path;
  return patterns
    .split(";")
    .map((p) => p.trim())
    .filter(Boolean)
    .some((p) => {
      const re = new RegExp(
        `^${p.replace(/[.+^${}()|[\]\\]/g, "\\$&").replace(/\*/g, ".*").replace(/\?/g, ".")}$`,
        "i",
      );
      return re.test(p.includes("/") ? path : name);
    });
}
