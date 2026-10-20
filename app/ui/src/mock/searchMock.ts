/**
 * Browser-mode search backend: `fs_find_files` over the demo project, plus Classes,
 * Symbols and Text providers that scan the demo PHP sources with regexes. The PHP pack
 * replaces the class/symbol providers with real index-backed ones (same ids not required:
 * these only register outside Tauri).
 */
import { isTauri, registerMock } from "@/ipc";
import type { FileMatch } from "@/ipc/search";
import { openFile } from "@/state/editor";
import { workspace } from "@/state/workspace";
import { fuzzyMatch, matchFilePath } from "@/features/search/fuzzy";
import { registerSearchProvider, type SearchItem, type SearchItemKind } from "@/features/search/providers";
import { demoFiles, demoProjectName } from "./demoProject";

registerMock("fs_find_files", ({ query, limit }) => {
  const q = String(query ?? "").trim();
  if (!q) return [];
  const hits: FileMatch[] = [];
  for (const path of Object.keys(demoFiles)) {
    const m = matchFilePath(q, path);
    if (m) hits.push({ path, name: path.split("/").pop()!, score: m.score, positions: m.positions, matchedPath: m.matchedPath });
  }
  hits.sort((a, b) => b.score - a.score || a.path.localeCompare(b.path));
  return hits.slice(0, Number(limit ?? 100));
});

interface DemoSymbol {
  name: string;
  kind: SearchItemKind;
  /** Namespace (classes) or containing class (members). */
  container: string;
  path: string;
  line: number;
}

/** Tiny regex "indexer" over the demo PHP files. */
export function indexDemoSymbols(files: Record<string, string>): DemoSymbol[] {
  const out: DemoSymbol[] = [];
  for (const [path, src] of Object.entries(files)) {
    if (!path.endsWith(".php")) continue;
    const ns = /^namespace\s+([^;]+);/m.exec(src)?.[1] ?? "";
    let cls = "";
    src.split("\n").forEach((text, i) => {
      const line = i + 1;
      const c = /^\s*(?:final\s+|abstract\s+|readonly\s+)*(class|interface|trait|enum)\s+(\w+)/.exec(text);
      if (c) {
        cls = c[2];
        out.push({ name: c[2], kind: c[1] as SearchItemKind, container: ns, path, line });
        return;
      }
      const f = /^\s*(?:(?:public|protected|private|static|final|abstract)\s+)*function\s+(\w+)/.exec(text);
      if (f) {
        out.push({ name: f[1], kind: cls ? "method" : "function", container: cls || ns, path, line });
        return;
      }
      const p = /^\s*(?:public|protected|private)\s+(?:static\s+|readonly\s+)*(?:[\w?\\|]+\s+)?\$(\w+)/.exec(text);
      if (p && cls) out.push({ name: `$${p[1]}`, kind: "property", container: cls, path, line });
      const k = /^\s*(?:(?:public|protected|private)\s+)?const\s+(\w+)/.exec(text) ?? /^\s*case\s+(\w+)\s*=/.exec(text);
      if (k && cls) out.push({ name: k[1], kind: "constant", container: cls, path, line });
    });
  }
  return out;
}

const VENDOR_CLASSES: DemoSymbol[] = [
  ["Model", "Illuminate\\Database\\Eloquent"],
  ["Collection", "Illuminate\\Support"],
  ["JsonResponse", "Illuminate\\Http"],
  ["Request", "Illuminate\\Http"],
  ["Route", "Symfony\\Component\\Routing\\Attribute"],
  ["Authenticatable", "Illuminate\\Foundation\\Auth"],
  ["HasMany", "Illuminate\\Database\\Eloquent\\Relations"],
  ["BelongsTo", "Illuminate\\Database\\Eloquent\\Relations"],
  ["TestCase", "PHPUnit\\Framework"],
].map(([name, container]) => ({ name, kind: "class" as const, container, path: "", line: 0 }));

if (!isTauri()) {
  const symbols = indexDemoSymbols(demoFiles);
  const open = (s: DemoSymbol) => () => (s.path ? openFile(s.path) : undefined);
  const toItem = (s: DemoSymbol, score: number, matches: number[], nonProject = false): SearchItem => ({
    id: `${s.path}:${s.line}:${s.name}`,
    label: s.name,
    matches,
    detail: s.kind === "method" || s.kind === "property" || s.kind === "constant" ? `${s.container}` : s.container,
    hint: nonProject ? "vendor" : (workspace()?.name ?? demoProjectName),
    kind: s.kind,
    score,
    run: open(s),
  });

  registerSearchProvider({
    id: "demo.classes",
    tab: "classes",
    title: "Classes",
    search(query, { includeNonProject, limit }) {
      if (!query.trim()) return [];
      const pool = [...symbols.filter((s) => ["class", "interface", "trait", "enum"].includes(s.kind)), ...(includeNonProject ? VENDOR_CLASSES : [])];
      return pool
        .map((s) => ({ s, m: fuzzyMatch(query, s.name, { strict: true }) }))
        .filter((x) => x.m)
        .map((x) => toItem(x.s, x.m!.score, x.m!.positions, !x.s.path))
        .sort((a, b) => b.score - a.score)
        .slice(0, limit);
    },
  });

  registerSearchProvider({
    id: "demo.symbols",
    tab: "symbols",
    title: "Symbols",
    search(query, { limit }) {
      if (!query.trim()) return [];
      return symbols
        .map((s) => ({ s, m: fuzzyMatch(query, s.name, { strict: true }) }))
        .filter((x) => x.m)
        .map((x) => toItem(x.s, x.m!.score - 2, x.m!.positions))
        .sort((a, b) => b.score - a.score)
        .slice(0, limit);
    },
  });

  registerSearchProvider({
    id: "demo.text",
    tab: "text",
    title: "Text",
    search(query, { limit }) {
      const q = query.trim().toLowerCase();
      if (q.length < 2) return [];
      const out: SearchItem[] = [];
      for (const [path, src] of Object.entries(demoFiles)) {
        src.split("\n").forEach((text, i) => {
          const at = text.toLowerCase().indexOf(q);
          if (at < 0 || out.length >= limit) return;
          const trimmed = text.trimStart();
          const shift = text.length - trimmed.length;
          out.push({
            id: `${path}:${i + 1}`,
            label: trimmed,
            matches: Array.from({ length: q.length }, (_, k) => at - shift + k),
            hint: `${path.split("/").pop()} ${i + 1}`,
            kind: "text",
            score: -i / 1000,
            run: () => openFile(path),
          });
        });
      }
      return out;
    },
  });
}
