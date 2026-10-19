import type { Extension } from "@codemirror/state";
import { php } from "@codemirror/lang-php";

/** Built-in syntax modes by file extension. Language packs can extend this later. */
const byExtension: Record<string, () => Extension> = {
  php: () => php(),
};

export function languageFor(path: string): Extension[] {
  if (path.endsWith(".blade.php")) return [php({ plain: false })];
  const ext = path.split(".").pop()?.toLowerCase() ?? "";
  const lang = byExtension[ext];
  return lang ? [lang()] : [];
}

export function languageLabel(path: string): string {
  const ext = path.split(".").pop()?.toLowerCase() ?? "";
  return ({ php: "PHP", json: "JSON", md: "Markdown", js: "JavaScript", ts: "TypeScript", lua: "Lua" } as Record<string, string>)[ext] ?? "Plain text";
}
