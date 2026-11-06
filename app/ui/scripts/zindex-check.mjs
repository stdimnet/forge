/**
 * `pnpm zindex:check`: every z-index above 10 in the UI uses the layer scale (a `--z-*`
 * variable of `src/theme/theme.css`), so popups, dialogs and notifications stack in one
 * order. `theme.css` itself defines the scale and is skipped. Exits with 1 on a violation.
 * The rule is in zindexCheck.mjs; the layering guide is
 * docs/guide/developer/contribution-points.md ("Layers").
 */
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative, resolve } from "node:path";
import { zIndexViolations } from "./zindexCheck.mjs";

const ui = resolve(import.meta.dirname, "..");
const src = join(ui, "src");
const SKIP = new Set([join(src, "theme", "theme.css")]);

function walk(dir, files = []) {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) {
      if (name === "pkg" || name === "node_modules") continue;
      walk(p, files);
    } else if (/\.(css|ts|tsx)$/.test(name) && !/\.test\.tsx?$/.test(name) && !name.endsWith(".d.ts") && !SKIP.has(p)) {
      files.push(p);
    }
  }
  return files;
}

let count = 0;
const files = walk(src);
for (const file of files) {
  for (const v of zIndexViolations(readFileSync(file, "utf8"), file.endsWith(".css"))) {
    count++;
    console.error(`${relative(ui, file)}:${v.line}: z-index ${v.value} is above 10; use a --z-* layer variable from theme.css`);
  }
}
if (count) {
  console.error(`\n${count} raw z-index value(s). See docs/guide/developer/contribution-points.md ("Layers").`);
  process.exit(1);
}
console.log(`z-index layers ok (${files.length} files)`);
