/**
 * Settings dialog search, computed in Rust (`forge_plugin_api::settings_model`, the
 * `settings_search` command): the page tree, the pages and settings that match, and the
 * highlighted segments of every text the dialog shows. The UI only renders the answer.
 */
import { createEffect, createRoot, createSignal, on, type Accessor } from "solid-js";
import {
  settingsApi,
  type HighlightSegment,
  type SettingsSearchRequest,
  type SettingsSearchResponse,
  type SettingsTreeNode,
} from "@/ipc/settings";

export type { SettingsTreeNode as TreeNode, HighlightSegment as Segment } from "@/ipc/settings";

export const EMPTY_SEARCH: SettingsSearchResponse = {
  tree: [],
  visible: [],
  matches: [],
  firstMatch: null,
  breadcrumbs: {},
  settings: [],
  entries: [],
  highlights: [],
};

export interface SearchResult {
  response: SettingsSearchResponse;
  /** Whether a response arrived (as opposed to the empty initial value). */
  ready: boolean;
  /** Highlighted segments by text. */
  segments: Map<string, HighlightSegment[]>;
}

/**
 * Re-run the search whenever `request` changes (`null` pauses it); answers to outdated
 * requests are dropped.
 */
export function createSettingsSearch(request: () => SettingsSearchRequest | null): Accessor<SearchResult> {
  const [result, setResult] = createSignal<SearchResult>({ response: EMPTY_SEARCH, ready: false, segments: new Map() });
  let seq = 0;
  createEffect(
    on(request, (req) => {
      if (!req) return;
      const id = ++seq;
      settingsApi
        .search(req)
        .then((response) => {
          if (id !== seq) return;
          const texts = req.highlight ?? [];
          setResult({ response, ready: true, segments: new Map(texts.map((t, i) => [t, response.highlights[i]])) });
        })
        .catch((err) => console.warn("settings search failed", err));
    }),
  );
  return result;
}

/** Segments of `text` in a result, or the plain text while none are known. */
export const segmentsOf = (result: SearchResult, text: string): HighlightSegment[] =>
  result.segments.get(text) ?? [{ text, hit: false }];

/** Index the nodes of a tree by id. */
export function nodesById(tree: SettingsTreeNode[]): Map<string, SettingsTreeNode> {
  const out = new Map<string, SettingsTreeNode>();
  const walk = (nodes: SettingsTreeNode[]) =>
    nodes.forEach((n) => {
      out.set(n.id, n);
      walk(n.children);
    });
  walk(tree);
  return out;
}

/** A search that lives as long as the app (the dialog's own). */
export const createGlobalSettingsSearch = (request: () => SettingsSearchRequest | null) =>
  createRoot(() => createSettingsSearch(request));
