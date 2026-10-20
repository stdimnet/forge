import { createSignal } from "solid-js";
import type { SearchTab } from "./providers";

export type SeTab = "all" | SearchTab;

const [seOpen, setSeOpen] = createSignal(false);
const [seTab, setSeTab] = createSignal<SeTab>("all");
const [seQuery, setSeQuery] = createSignal("");
const [seIncludeNonProject, setSeIncludeNonProject] = createSignal(false);

export { seOpen, seTab, seQuery, seIncludeNonProject, setSeTab, setSeQuery, setSeIncludeNonProject };

/**
 * Open Search Everywhere on a tab. Invoking the same tab again while open toggles
 * "Include non-project items", like pressing Ctrl+N twice in PHPStorm.
 */
export function openSearchEverywhere(tab: SeTab = "all", query?: string): void {
  if (seOpen() && seTab() === tab && query === undefined) {
    setSeIncludeNonProject((v) => !v);
    return;
  }
  setSeTab(tab);
  if (query !== undefined) setSeQuery(query);
  setSeOpen(true);
}

export function closeSearchEverywhere(): void {
  setSeOpen(false);
  setSeIncludeNonProject(false);
}
