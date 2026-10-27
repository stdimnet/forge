/**
 * RustForge in the editor:
 *
 * - ▶ on `fn main` of binaries and examples (Run / Debug / Modify Run Configuration,
 *   `rust_file_targets`); tests, test modules and doctests get their ▶ from the shared test
 *   runner (`cargo-test`),
 * - Reformat Code with rustfmt (`rust.formatter`, the project's rustfmt.toml),
 * - Clippy as the external linter: problems in the editor and the Problems tool window,
 *   its suggestions as quick fixes (Alt+Enter), on demand and on save
 *   (`rust.clippy.onSave`).
 */
import { Bug, Play, Settings2 } from "lucide-solid";
import type { EditorDiagnostic } from "@/ipc/lsp";
import { rustApi, type RustProblem } from "@/ipc/rust";
import type { ContextAction } from "@/ipc/codeintel";
import { registerLineMarkerProvider } from "@/editor/lineMarkers";
import { registerContextActionProvider } from "@/features/codeintel/intentions";
import { publishDiagnostics } from "@/features/lsp/store";
import { registerFormatProvider } from "@/features/lsp/format";
import { openMenu } from "@/features/run/PopupMenu";
import { useTemporary } from "@/features/run/configStore";
import { debugConfiguration, launch } from "@/features/run/runStore";
import { openRunConfigurations } from "@/features/run/dialogState";
import { shortcutLabel } from "@/keymap";
import { onFileSaved } from "@/state/editor";
import { getSetting } from "@/state/settings";
import { cargoWorkspace, rustActive, showRustBalloon } from "./state";

export const isRustPath = (path: string) => path.endsWith(".rs");

// ---- ▶ on fn main -----------------------------------------------------------------------------

registerLineMarkerProvider({
  appliesTo: isRustPath,
  watch: () => rustActive(),
  markers: async (path, text) => {
    if (!rustActive() || !/\bfn\s+main\s*\(/.test(text)) return [];
    const targets = await rustApi.fileTargets(path, text).catch(() => []);
    return targets.map((t) => {
      const label = t.name || "main";
      return {
        line: t.line,
        icon: Play,
        title: `Run '${label}'`,
        className: "marker-run",
        onClick: (_view, event) => {
          const cfg = t.configuration;
          openMenu({
            x: event.clientX,
            y: event.clientY,
            items: [
              { label: `Run '${label}'`, icon: Play, iconColor: "var(--success)", shortcut: shortcutLabel("RunClass"), run: () => void launch(useTemporary(cfg)) },
              { label: `Debug '${label}'`, icon: Bug, iconColor: "var(--success)", shortcut: shortcutLabel("DebugClass"), run: () => void debugConfiguration(useTemporary(cfg)) },
              { separator: true },
              { label: "Modify Run Configuration…", icon: Settings2, run: () => openRunConfigurations(useTemporary(cfg).name) },
            ],
          });
        },
      };
    });
  },
});

// ---- Reformat Code ---------------------------------------------------------------------------

registerFormatProvider(async (path, text) => {
  if (!isRustPath(path) || !rustActive()) return null;
  const formatter = getSetting<string>("rust.formatter", "rustfmt");
  if (formatter === "none") return text;
  if (formatter === "lsp") return null;
  try {
    return (await rustApi.format(path, text)) ?? null;
  } catch (e) {
    showRustBalloon("rustfmt", String(e).replace(/^Error: /, ""), 6000);
    return text;
  }
});

// ---- Clippy -----------------------------------------------------------------------------------

const SOURCE = "clippy";
/** The last problems by file, for quick fixes; files that had some, to clear them. */
const problemsByFile = new Map<string, RustProblem[]>();

/** Publish Clippy's problems; files of `scope` (a package folder, or everything) without problems are cleared. */
export function publishClippy(problems: RustProblem[], scope?: string): void {
  const byFile = new Map<string, RustProblem[]>();
  for (const p of problems) byFile.set(p.path, [...(byFile.get(p.path) ?? []), p]);
  const inScope = (f: string) => scope === undefined || f.startsWith(scope);
  for (const f of [...problemsByFile.keys()]) {
    if (inScope(f) && !byFile.has(f)) {
      publishDiagnostics(f, [], SOURCE);
      problemsByFile.delete(f);
    }
  }
  for (const [file, list] of byFile) {
    problemsByFile.set(file, list);
    publishDiagnostics(
      file,
      list.map(
        (p): EditorDiagnostic => ({
          range: { start: { line: p.line, character: p.column }, end: { line: p.endLine, character: p.endColumn } },
          severity: p.severity,
          message: p.message,
          source: p.source === "clippy" ? "Clippy" : "rustc",
          code: p.code?.replace(/^clippy::/, ""),
        }),
      ),
      SOURCE,
    );
  }
}

/** Run Clippy for a file's package (the whole workspace without a path). */
export async function runClippy(path?: string, quiet = false): Promise<RustProblem[] | null> {
  try {
    const problems = await rustApi.clippy(path);
    // A file's run covers its package: problems of other packages stay.
    const pkg = path
      ? cargoWorkspace()
          ?.members.filter((m) => !m.dir || path.startsWith(`${m.dir}/`))
          .sort((a, b) => b.dir.length - a.dir.length)[0]
      : undefined;
    publishClippy(problems, pkg ? (pkg.dir ? `${pkg.dir}/` : "") : undefined);
    if (!quiet) showRustBalloon("Clippy", problems.length ? `${problems.length} problem${problems.length === 1 ? "" : "s"} found` : "No problems found", 4000);
    return problems;
  } catch (e) {
    if (!quiet) showRustBalloon("Clippy", String(e).replace(/^Error: /, ""), 8000);
    return null;
  }
}

/** Alt+Enter: Clippy's suggestions at the caret as quick fixes. */
export function clippyFixes(path: string, line: number, column: number): ContextAction[] {
  const list = problemsByFile.get(path) ?? [];
  const here = list.filter((p) => line >= p.line && line <= p.endLine && (line !== p.line || column >= p.column) && (line !== p.endLine || column <= p.endColumn));
  const candidates = here.length ? here : list.filter((p) => p.line === line);
  return candidates.flatMap((p) =>
    (p.fixes ?? []).map(
      (fix, i): ContextAction => ({
        title: fix.title,
        kind: "quickFix",
        source: "clippy",
        isPreferred: i === 0 && fix.applicability === "MachineApplicable",
        needsResolve: false,
        edit: {
          changes: [...new Set(fix.edits.map((e) => e.path))].map((file) => ({
            kind: "edit" as const,
            path: file,
            edits: fix.edits
              .filter((e) => e.path === file)
              .map((e) => ({ range: { start: { line: e.line, character: e.column }, end: { line: e.endLine, character: e.endColumn } }, newText: e.text })),
          })),
        },
      }),
    ),
  );
}

registerContextActionProvider(async (path, view) => {
  if (!isRustPath(path) || !rustActive()) return [];
  const pos = view.state.selection.main.head;
  const line = view.state.doc.lineAt(pos);
  return clippyFixes(path, line.number - 1, pos - line.from);
});

if (typeof document !== "undefined") {
  onFileSaved((path) => {
    if (!isRustPath(path) || !rustActive()) return;
    if (getSetting<boolean>("rust.clippy.onSave", false)) void runClippy(path, true);
  });
}
