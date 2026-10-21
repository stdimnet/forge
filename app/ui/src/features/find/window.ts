/** Find tool window: Find in Files results without the popup's match cap (setting `search.findInFiles.maxMatches`). */
import { createSignal } from "solid-js";
import { createStore } from "solid-js/store";
import { findApi, type FileMatches, type FindRequest } from "@/ipc/find";
import { searchApi } from "@/ipc/search";
import { getSetting } from "@/state/settings";
import { showToolWindow } from "@/state/layout";

export const FIND_TOOL_WINDOW = "find";

/** What "no limit" means in the request: a pattern like `.*` matches every line of the project. */
const UNLIMITED = 1_000_000;
/** Rows fetched per request; the rest follows while the list is scrolled. */
const PAGE_ROWS = 2000;

export interface FindForm {
  query: string;
  caseSensitive: boolean;
  wholeWords: boolean;
  regex: boolean;
  fileMask: string;
  directory: string;
}

export const [findForm, setFindForm] = createStore<FindForm>({
  query: "",
  caseSensitive: false,
  wholeWords: false,
  regex: false,
  fileMask: "",
  directory: "",
});

export interface FindWindowResults {
  searched: boolean;
  /** Loaded part of the result (the first pages). */
  files: FileMatches[];
  matchCount: number;
  fileCount: number;
  rowCount: number;
  truncated: boolean;
  error: string | null;
  busy: boolean;
  indexing: boolean;
  /** How long the search took, until the first rows were back. */
  tookMs: number;
}

const empty: FindWindowResults = {
  searched: false,
  files: [],
  matchCount: 0,
  fileCount: 0,
  rowCount: 0,
  truncated: false,
  error: null,
  busy: false,
  indexing: false,
  tookMs: 0,
};

export const [findResults, setFindResults] = createSignal<FindWindowResults>(empty);

/** Files whose matches are folded away in the results tree. */
export const [collapsedFiles, setCollapsedFiles] = createSignal<Set<string>>(new Set());

let latest = 0;
let resultId = 0;
let loadedRows = 0;
let loadingMore = false;

/** Whether rows of the current result are still to be fetched. */
export const hasMoreRows = () => loadedRows < findResults().rowCount;

const countRows = (files: FileMatches[]) => files.reduce((n, f) => n + f.matches.length, 0);

/** Append a page; a file cut by the page edge continues in the last loaded file. */
function appendPage(files: FileMatches[], page: FileMatches[]): FileMatches[] {
  const out = files.slice();
  for (const file of page) {
    const last = out[out.length - 1];
    if (last && last.path === file.path) out[out.length - 1] = { ...last, matches: last.matches.concat(file.matches) };
    else out.push(file);
  }
  return out;
}

export async function runFindWindow(): Promise<void> {
  if (!findForm.query) return;
  const maxMatches = getSetting<number>("search.findInFiles.maxMatches", 0);
  const request: FindRequest = { ...findForm, limit: maxMatches > 0 ? maxMatches : UNLIMITED };
  const mine = ++latest;
  const started = performance.now();
  loadedRows = 0;
  loadingMore = false;
  setCollapsedFiles(new Set<string>());
  setFindResults({ ...empty, searched: true, busy: true });
  try {
    const [paged, status] = await Promise.all([findApi.searchPaged(request, PAGE_ROWS), searchApi.status().catch(() => null)]);
    if (mine !== latest) return;
    resultId = paged.id;
    loadedRows = countRows(paged.files);
    setFindResults({
      searched: true,
      files: paged.files,
      matchCount: paged.matchCount,
      fileCount: paged.fileCount,
      rowCount: paged.rowCount,
      truncated: paged.truncated,
      error: paged.error ?? null,
      busy: false,
      indexing: status?.indexing ?? false,
      tookMs: performance.now() - started,
    });
  } catch (e) {
    if (mine === latest) setFindResults({ ...empty, searched: true, error: String(e instanceof Error ? e.message : e) });
  }
}

/** Fetch the next page of the current result (called while the list is scrolled near its end). */
export async function loadMoreRows(): Promise<void> {
  if (loadingMore || !hasMoreRows()) return;
  loadingMore = true;
  const mine = latest;
  try {
    const page = await findApi.page(resultId, loadedRows, PAGE_ROWS);
    if (mine !== latest) return;
    loadedRows += countRows(page);
    setFindResults((r) => ({ ...r, files: appendPage(r.files, page) }));
  } catch (e) {
    if (mine === latest) setFindResults((r) => ({ ...r, error: String(e instanceof Error ? e.message : e) }));
  } finally {
    loadingMore = false;
  }
}

/** Show the Find tool window and search with `init` merged into the form. */
export function openFindWindow(init?: Partial<FindForm>): void {
  if (init) setFindForm(init);
  showToolWindow("bottom", FIND_TOOL_WINDOW);
  void runFindWindow();
}
