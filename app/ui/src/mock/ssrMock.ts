/**
 * Browser-mode Structural Search and Replace. The template library and template variables
 * come from Rust (`forge-ssr` without its engine, WebAssembly); the tree-sitter matcher is C
 * code that does not build to WebAssembly, so searches over the in-memory demo project use
 * a regular-expression approximation of the template here. The desktop app always matches
 * syntax trees in Rust.
 */
import { registerMock } from "@/ipc";
import type { ReplaceDone } from "@/ipc/find";
import type { SsrMatch, SsrRequest, SsrResult, SsrSavedTemplate, SsrTemplateCheck } from "@/ipc/ssr";
import { callWasm, forgeWasm } from "@/wasm";
import { projectFiles } from "./findMock";
import { rememberReplace } from "./replaceMock";
import { mockFiles } from "./workspaceMock";

registerMock("ssr_templates", async () => {
  const w = await forgeWasm();
  return callWasm<SsrSavedTemplate[]>(w.ssr_templates, {});
});

registerMock("ssr_check", async ({ request }) => {
  const w = await forgeWasm();
  const r = request as SsrRequest;
  const check = await callWasm<SsrTemplateCheck>(w.ssr_variables, { template: r.template, replacement: r.replacement });
  const open = (r.template.match(/[([{]/g) ?? []).length;
  const close = (r.template.match(/[)\]}]/g) ?? []).length;
  if (open !== close) check.error = `The search template is not a valid ${r.fileType === "php" ? "PHP" : r.fileType === "javaScript" ? "JavaScript" : "TypeScript"} code fragment`;
  return check;
});

const EXTENSIONS: Record<string, string[]> = { php: ["php"], javaScript: ["js", "jsx", "mjs", "cjs"], typeScript: ["ts", "tsx"] };

/** The template as a regular expression: variables match a name, a call or an argument list. */
function templateRegex(req: SsrRequest): { re: RegExp; names: string[] } {
  const names: string[] = [];
  let source = "";
  const parts = req.template.trim().replace(/;\s*$/, "").split(/(\$[A-Za-z_]\w*\$)/);
  for (let i = 0; i < parts.length; i++) {
    const part = parts[i];
    const v = /^\$([A-Za-z_]\w*)\$$/.exec(part);
    if (!v) {
      source += part.replace(/[.*+?^${}()|[\]\\]/g, "\\$&").replace(/\s+/g, "\\s*");
      continue;
    }
    const name = v[1];
    const c = req.constraints?.find((x) => x.name === name);
    const many = c?.maxCount === null || (c?.maxCount ?? 1) > 1;
    const optional = (c?.minCount ?? 1) === 0;
    if (names.includes(name)) {
      source += `\\k<${name}>`;
      continue;
    }
    names.push(name);
    const atom = String.raw`\$?[A-Za-z_\\][\w\\]*(?:\([^()]*\))?(?:->\w+(?:\([^()]*\))?)*|'[^']*'|"[^"]*"|\d+|\[[^\]]*\]`;
    const body = many ? `[^()]*?` : `(?:${atom})`;
    source += `(?<${name}>${body}${optional && !many ? "|" : ""})`;
  }
  return { re: new RegExp(source, req.matchCase === false ? "gi" : "g"), names };
}

function fill(template: string, groups: Record<string, string | undefined>): string {
  return template.replace(/\$([A-Za-z_]\w*)\$/g, (whole, n: string) => groups[n] ?? whole);
}

async function search(req: SsrRequest): Promise<SsrResult> {
  const { re } = templateRegex(req);
  const exts = EXTENSIONS[req.fileType] ?? ["php"];
  let files = projectFiles().filter(([p]) => exts.includes(p.split(".").pop() ?? "") && !p.endsWith(".blade.php"));
  if (req.scope) {
    const w = await forgeWasm();
    const inScope = await callWasm<boolean[]>(w.scopes_contains, { filter: req.scope, paths: files.map(([p]) => p) });
    files = files.filter((_, i) => inScope[i]);
  }
  const result: SsrResult = { files: [], matchCount: 0, searchedFiles: files.length, truncated: false };
  for (const [path, disk] of files) {
    const text = req.buffers?.[path] ?? disk;
    const matches: SsrMatch[] = [];
    for (const m of text.matchAll(re)) {
      if (!m[0]) continue;
      const start = m.index ?? 0;
      const before = text.slice(0, start);
      const line = before.split("\n").length - 1;
      const lineStart = before.lastIndexOf("\n") + 1;
      const lineText = text.slice(lineStart).split("\n")[0];
      const indent = lineText.length - lineText.trimStart().length;
      const column = start - lineStart;
      const endText = text.slice(0, start + m[0].length);
      const groups = m.groups ?? {};
      matches.push({
        line,
        column,
        endLine: endText.split("\n").length - 1,
        endColumn: start + m[0].length - (endText.lastIndexOf("\n") + 1),
        start,
        end: start + m[0].length,
        text: lineText.trimStart(),
        ranges: [[column - indent, Math.min(lineText.length, column + m[0].length) - indent]],
        matched: m[0],
        bindings: Object.entries(groups).map(([name, t]) => ({ name, text: t ?? "" })),
        replacement: req.replacement !== undefined ? fill(req.replacement, groups) : undefined,
      });
    }
    if (matches.length) {
      result.files.push({ path, matches });
      result.matchCount += matches.length;
    }
  }
  return result;
}

registerMock("ssr_search", ({ request }) => search(request as SsrRequest));

registerMock("ssr_replace", async ({ request }): Promise<ReplaceDone> => {
  const req = request as SsrRequest;
  const found = await search(req);
  const changed: { path: string; before: string; after: string }[] = [];
  let replacements = 0;
  for (const f of found.files) {
    const before = mockFiles[f.path];
    let after = before;
    for (const m of [...f.matches].reverse()) after = after.slice(0, m.start) + (m.replacement ?? m.matched) + after.slice(m.end);
    mockFiles[f.path] = after;
    changed.push({ path: f.path, before, after });
    replacements += f.matches.length;
  }
  rememberReplace(changed);
  return { files: changed.length, replacements, skipped: [] };
});
