/**
 * Structural Search and Replace (Edit | Find | Search Structurally… / Replace
 * Structurally…): the dialog's form, the request it sends and the results in the Find tool
 * window. Matching happens in Rust (`forge-ssr`) through the project's backend.
 */
import { createSignal } from "solid-js";
import { createStore, reconcile } from "solid-js/store";
import { ssrApi, type SsrConstraint, type SsrFileType, type SsrRequest, type SsrResult } from "@/ipc/ssr";
import type { UsagesAnswer } from "@/ipc/codeintel";
import { findApi } from "@/ipc/find";
import { activeSelection } from "@/editor/views";
import { editor, isDirty, saveAll } from "@/state/editor";
import { balloonsFor } from "@/keymap";
import { openInFindWindow, closeFindTab, type FindTab } from "@/features/codeintel/usages";
import { scopeFilter } from "@/features/scopes/model";
import { t as tr } from "@/i18n";

const showBalloon = balloonsFor("Find");

export type SsrMode = "search" | "replace";

export interface SsrForm {
  template: string;
  replacement: string;
  fileType: SsrFileType;
  matchCase: boolean;
  scopeName: string;
  constraints: Record<string, SsrConstraint>;
}

export const FILE_TYPES: { value: SsrFileType; label: string; ext: string }[] = [
  { value: "php", label: "PHP", ext: "php" },
  { value: "javaScript", label: "JavaScript", ext: "js" },
  { value: "typeScript", label: "TypeScript", ext: "ts" },
];

const [form, setForm] = createStore<SsrForm>({
  template: "$Instance$->$MethodCall$($Parameter$)",
  replacement: "$Instance$->$MethodCall$($Parameter$)",
  fileType: "php",
  matchCase: true,
  scopeName: "Project Files",
  constraints: { Parameter: { name: "Parameter", minCount: 0, maxCount: null } },
});
export { form as ssrForm, setForm as setSsrForm };

const [dialog, setDialog] = createSignal<SsrMode | null>(null);
export { dialog as ssrDialog };
export const closeSsrDialog = () => setDialog(null);

/** The file type of a path, for the dialog's default. */
function fileTypeOf(path: string | null | undefined): SsrFileType | undefined {
  const ext = path?.split(".").pop()?.toLowerCase() ?? "";
  if (["php", "phtml", "inc"].includes(ext)) return "php";
  if (["js", "jsx", "mjs", "cjs"].includes(ext)) return "javaScript";
  if (["ts", "tsx", "mts", "cts"].includes(ext)) return "typeScript";
  return undefined;
}

/** Search Structurally… / Replace Structurally…: the selection becomes the template. */
export function openSsrDialog(mode: SsrMode): void {
  const type = fileTypeOf(editor.active);
  if (type && type !== form.fileType) setForm("fileType", type);
  const selected = activeSelection()?.text;
  if (selected?.trim()) {
    setForm({ template: selected.trim(), constraints: {} });
    if (mode === "replace") setForm("replacement", selected.trim());
  }
  setDialog(mode);
}

/** Apply a saved template from the library. */
export function useTemplate(t: { template: string; fileType: SsrFileType; constraints: SsrConstraint[]; replacement?: string }): void {
  setForm({
    template: t.template,
    replacement: t.replacement ?? t.template,
    fileType: t.fileType,
  });
  setForm("constraints", reconcile(Object.fromEntries(t.constraints.map((c) => [c.name, c]))));
}

const UNLIMITED = 100_000;

/** Unsaved editor contents, so matches are what is on screen. */
function dirtyBuffers(): Record<string, string> {
  return Object.fromEntries(editor.tabs.filter((t) => isDirty(t) && !t.path.includes("://")).map((t) => [t.path, t.content]));
}

/** The request for the form; `replace` adds the replacement template. */
export function ssrRequest(f: SsrForm, replace: boolean, limit = 5000): SsrRequest {
  return {
    template: f.template,
    fileType: f.fileType,
    constraints: Object.values(f.constraints),
    matchCase: f.matchCase,
    replacement: replace ? f.replacement : undefined,
    scope: f.scopeName === "Project Files" ? undefined : scopeFilter(f.scopeName),
    buffers: dirtyBuffers(),
    limit,
  };
}

/** Structural search results in the Find tool window's shape (one group, files, lines). */
export function ssrAnswer(title: string, r: SsrResult): UsagesAnswer {
  const files = r.files.map((f) => ({
    path: f.path,
    usages: f.matches.map((m) => ({
      line: m.line,
      column: m.column,
      endLine: m.endLine,
      endColumn: m.endColumn,
      text: m.text,
      ranges: m.ranges,
      kind: "unclassified" as const,
      after: m.replacement,
    })),
  }));
  const tree = { total: r.matchCount, fileCount: files.length, groups: [{ label: "Found Occurrences", count: r.matchCount, files }] };
  return { title, source: "none", byType: tree, byFile: tree };
}

const firstLine = (t: string) => {
  const line = t.trim().split("\n")[0];
  return line.length > 60 ? `${line.slice(0, 57)}…` : line;
};

/** Find / Replace in the dialog: the matches as a tab of the Find tool window. */
export async function runSsr(mode: SsrMode): Promise<boolean> {
  const f = JSON.parse(JSON.stringify(form)) as SsrForm;
  const replace = mode === "replace";
  const search = async () => {
    const r = await ssrApi.search(ssrRequest(f, replace));
    if (r.error) throw new Error(r.error);
    return r;
  };
  let result: SsrResult;
  try {
    result = await search();
  } catch (e) {
    showBalloon(tr("tools.ssr.structuralSearchFailed"), String(e instanceof Error ? e.message : e));
    return false;
  }
  const where = f.scopeName;
  if (!result.matchCount) {
    showBalloon(tr("tools.ssr.noOccurrencesOfFoundIn", { value: firstLine(f.template), where }));
    return false;
  }
  const title = firstLine(f.template);
  openInFindWindow(ssrAnswer(title, result), {
    kind: "text",
    newTab: true,
    title: replace ? `Replace '${title}'` : `Search '${title}'`,
    header: `${replace ? "Replace" : "Search"} structurally for '${title}' in ${where}`,
    rerun: async () => ssrAnswer(title, await search()),
    actions: replace
      ? [
          { label: "Replace All", primary: true, run: (tab) => replaceAll(tab, f) },
          { label: "Cancel", run: (tab) => closeFindTab(tab.id) },
        ]
      : undefined,
  });
  return true;
}

/** Replace All: every match replaced through the backend; Undo puts the files back. */
async function replaceAll(tab: FindTab, f: SsrForm): Promise<void> {
  await saveAll().catch(() => undefined);
  const done = await ssrApi.replace({ ...ssrRequest(f, true, UNLIMITED), buffers: undefined });
  closeFindTab(tab.id);
  const skipped = done.skipped.length ? `, ${done.skipped.length} skipped` : "";
  showBalloon(
    tr("tools.ssr.occurrenceReplacedInFile", { replacements: done.replacements, value: done.replacements, files: done.files, value2: done.files, skipped }),
    undefined,
    8000,
    [
      {
        label: "Undo",
        run: () =>
          void findApi.undoReplace().then((u) =>
            showBalloon(tr("tools.ssr.restoredFiles", { count: u.restored }), u.left.length ? tr("tools.ssr.changedSince", { left: u.left.join(", ") }) : undefined),
          ),
      },
    ],
  );
}
