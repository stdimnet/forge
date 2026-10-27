/**
 * ktlint and detekt in the editor and the Problems view: ktlint checks a Kotlin file when it
 * opens and after each save (on the editor text, through stdin), detekt checks the saved
 * file (and the whole project with "Run detekt on the Project"). Reports are parsed in Rust
 * (`kotlinforge_lang_kotlin::lint`). Reformat Code uses ktlint format when chosen in the
 * settings, and "Run ktlint format on save" formats after Ctrl+S.
 */
import type { EditorDiagnostic } from "@/ipc/lsp";
import { kotlinApi, type LintProblem } from "@/ipc/kotlin";
import { publishDiagnostics } from "@/features/lsp/store";
import { registerFormatProvider } from "@/features/lsp/format";
import { applyLineChanges, viewFor } from "@/editor/views";
import { balloonsFor } from "@/keymap/dispatch";
import { editor, onFileSaved, saveFile } from "@/state/editor";
import { getSetting } from "@/state/settings";
import { active } from "./store";

const showBalloon = balloonsFor("Kotlin");
const SOURCES = { ktlint: "kotlin-ktlint", detekt: "kotlin-detekt" } as const;

export const isKotlinSource = (path: string) => path.endsWith(".kt") || (path.endsWith(".kts") && !path.endsWith(".gradle.kts"));

function toDiagnostic(p: LintProblem): EditorDiagnostic {
  return {
    range: { start: { line: p.line, character: p.column }, end: { line: p.line, character: p.column + 1 } },
    severity: p.severity,
    message: p.message,
    source: p.linter,
    code: p.rule,
  };
}

/** Show the problems of one linter for `path` (replacing its earlier ones). */
export function publishLint(path: string, linter: LintProblem["linter"], problems: LintProblem[]): void {
  publishDiagnostics(path, problems.map(toDiagnostic), SOURCES[linter]);
}

let ktlintError = false;

/** ktlint on the editor text of `path`. */
export async function ktlintFile(path: string, text: string): Promise<void> {
  if (!active() || !isKotlinSource(path) || !getSetting<boolean>("kotlin.ktlint.enabled", true)) return;
  try {
    publishLint(path, "ktlint", await kotlinApi.lint(path, text));
    ktlintError = false;
  } catch (e) {
    // One balloon per session for a broken setup, not one per file.
    if (!ktlintError) showBalloon("ktlint failed", String(e).replace(/^Error: /, ""), 6000, undefined, { type: "warning" });
    ktlintError = true;
  }
}

/** detekt on files (or the project): their problems replace the earlier ones. */
export async function detektFiles(paths?: string[], announce = false): Promise<void> {
  if (!active()) return;
  try {
    const found = await kotlinApi.detekt(paths);
    const byFile = new Map<string, LintProblem[]>();
    for (const p of paths ?? []) byFile.set(p, []);
    for (const p of found) byFile.set(p.file, [...(byFile.get(p.file) ?? []), p]);
    for (const [file, list] of byFile) publishLint(file, "detekt", list);
    if (announce) showBalloon("detekt finished", found.length ? `${found.length} problems in ${new Set(found.map((p) => p.file)).size} files: see the Problems tool window.` : "No problems found.", 5000);
  } catch (e) {
    if (announce) showBalloon("detekt failed", String(e).replace(/^Error: /, ""), 8000, undefined, { type: "error" });
  }
}

/** ktlint format of a file's editor text, applied as one edit. */
export async function ktlintFormatFile(path: string): Promise<boolean> {
  const view = viewFor(path);
  if (!view) return false;
  const text = view.state.doc.toString();
  try {
    const formatted = await kotlinApi.format(path, text);
    if (formatted === null) {
      showBalloon("ktlint is not installed", "Download ktlint in Settings | Languages & Frameworks | Kotlin, or put it on the PATH.", 6000);
      return false;
    }
    if (formatted === text) return false;
    applyLineChanges(view, formatted);
    return true;
  } catch (e) {
    showBalloon("ktlint format failed", String(e).replace(/^Error: /, ""), 6000, undefined, { type: "error" });
    return false;
  }
}

registerFormatProvider(async (path, text) => {
  if (!active() || !isKotlinSource(path) || getSetting<string>("kotlin.formatter", "lsp") !== "ktlint") return null;
  try {
    return (await kotlinApi.format(path, text)) ?? null;
  } catch (e) {
    showBalloon("ktlint format failed", String(e).replace(/^Error: /, ""), 5000, undefined, { type: "error" });
    return text;
  }
});

const formatting = new Set<string>();

if (typeof document !== "undefined") {
  onFileSaved((path, content) => {
    if (!active() || !isKotlinSource(path) || formatting.has(path)) return;
    void (async () => {
      if (getSetting<boolean>("kotlin.ktlint.formatOnSave", false)) {
        formatting.add(path);
        try {
          if (await ktlintFormatFile(path)) await saveFile(path, "plain");
        } finally {
          formatting.delete(path);
        }
      }
      const tab = editor.tabs.find((t) => t.path === path);
      await ktlintFile(path, tab?.content ?? content);
      if (getSetting<boolean>("kotlin.detekt.enabled", true) && path.endsWith(".kt")) await detektFiles([path]);
    })();
  });
}
