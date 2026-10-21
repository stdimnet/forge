/** Find tool window: Find in Files results without the popup's match cap (setting `search.findInFiles.maxMatches`). */
import { createSignal } from "solid-js";
import { createStore } from "solid-js/store";
import { findApi, type FindRequest, type FindResult } from "@/ipc/find";
import { searchApi } from "@/ipc/search";
import { getSetting } from "@/state/settings";
import { showToolWindow } from "@/state/layout";

export const FIND_TOOL_WINDOW = "find";

/** Stands in for "no limit" in the request. */
const UNLIMITED = 100_000_000;

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
  result: FindResult | null;
  error: string | null;
  busy: boolean;
  indexing: boolean;
}

export const [findResults, setFindResults] = createSignal<FindWindowResults>({
  searched: false,
  result: null,
  error: null,
  busy: false,
  indexing: false,
});

/** Files whose matches are folded away in the results tree. */
export const [collapsedFiles, setCollapsedFiles] = createSignal<Set<string>>(new Set());

let latest = 0;

export async function runFindWindow(): Promise<void> {
  if (!findForm.query) return;
  const maxMatches = getSetting<number>("search.findInFiles.maxMatches", 0);
  const request: FindRequest = { ...findForm, limit: maxMatches > 0 ? maxMatches : UNLIMITED };
  const mine = ++latest;
  setCollapsedFiles(new Set<string>());
  setFindResults({ searched: true, result: null, error: null, busy: true, indexing: false });
  try {
    const [result, status] = await Promise.all([findApi.search(request), searchApi.status().catch(() => null)]);
    if (mine === latest) {
      setFindResults({ searched: true, result, error: result.error ?? null, busy: false, indexing: status?.indexing ?? false });
    }
  } catch (e) {
    if (mine === latest) {
      setFindResults({ searched: true, result: null, error: String(e instanceof Error ? e.message : e), busy: false, indexing: false });
    }
  }
}

/** Show the Find tool window and search with `init` merged into the form. */
export function openFindWindow(init?: Partial<FindForm>): void {
  if (init) setFindForm(init);
  showToolWindow("bottom", FIND_TOOL_WINDOW);
  void runFindWindow();
}
