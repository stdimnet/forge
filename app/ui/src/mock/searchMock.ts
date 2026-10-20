/**
 * Browser-mode search backend: `fs_find_files` over the demo project, plus Classes,
 * Symbols and Text providers that scan the demo PHP sources with regexes. The PHP pack
 * replaces the class/symbol providers with real index-backed ones (same ids not required:
 * these only register outside Tauri).
 */
import { isTauri, registerMock } from "@/ipc";
import { openFile } from "@/state/editor";
import { workspace } from "@/state/workspace";
import { registerSearchProvider, type SearchItem, type SearchItemKind } from "@/features/search/providers";
import { demoFiles, demoProjectName } from "./demoProject";
import { matchFiles } from "./fuzzyMock";

registerMock("fs_find_files", ({ query, limit }) =>
  matchFiles(String(query ?? ""), Object.keys(demoFiles), Number(limit ?? 100)),
);

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
  // Candidates are matched (strictly, like PHPStorm's Go to Class) and ranked in Rust.
  const toItem = (s: DemoSymbol, score = 0): SearchItem => ({
    id: `${s.path}:${s.line}:${s.name}`,
    label: s.name,
    match: { strict: true },
    detail: s.container,
    hint: s.path ? (workspace()?.name ?? demoProjectName) : "vendor",
    kind: s.kind,
    score,
    run: open(s),
  });

  registerSearchProvider({
    id: "demo.classes",
    tab: "classes",
    title: "Classes",
    search(query, { includeNonProject }) {
      if (!query.trim()) return [];
      const classes = symbols.filter((s) => ["class", "interface", "trait", "enum"].includes(s.kind));
      return [...classes, ...(includeNonProject ? VENDOR_CLASSES : [])].map((s) => toItem(s));
    },
  });

  registerSearchProvider({
    id: "demo.symbols",
    tab: "symbols",
    title: "Symbols",
    search(query) {
      if (!query.trim()) return [];
      return symbols.map((s) => toItem(s, -2));
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
            run: () => openFile(path),
          });
        });
      }
      return out;
    },
  });
}
