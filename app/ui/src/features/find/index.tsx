/**
 * Find in Files (Ctrl+Shift+F, PHPStorm's `FindInPath`) and the Text tab of Search
 * Everywhere, both answered by the Rust `find_in_files` command (`crates/forge-find`)
 * through the project's backend, so remote projects are searched on their host.
 */
import { render } from "solid-js/web";
import { Show } from "solid-js";
import { registerCommand } from "@/state/commands";
import { actionById } from "@/keymap";
import { activeSelection, openFileAt } from "@/editor/views";
import { findApi } from "@/ipc/find";
import { registerSearchProvider, type SearchItem } from "@/features/search/providers";
import { registerStatusItem } from "@/features/statusItems";
import { FindInFilesPopup } from "./FindInFiles";
import { IndexStatus } from "./IndexStatus";
import { findOpen, openFindInFiles } from "./state";
import { openFindWindow } from "./window";
import { t } from "@/i18n";
import "./find.css";
import "./findWindow.css";

registerCommand({
  id: "FindInPath",
  title: actionById("FindInPath")?.title ?? "Find in Files…",
  category: "Find",
  run: () => openFindInFiles(),
});

/**
 * Replace in Files (Ctrl+Shift+R): the Find tool window's Find in Files tab with the selection
 * as the query and the caret in the replacement field.
 */
registerCommand({
  id: "ReplaceInPath",
  title: actionById("ReplaceInPath")?.title ?? "Replace in Files…",
  category: "Find",
  run: () => {
    const selected = activeSelection()?.text;
    openFindWindow(selected && !selected.includes("\n") ? { query: selected } : undefined);
    queueMicrotask(() => document.querySelector<HTMLInputElement>(".fw-replace .fw-pattern")?.focus());
  },
});

registerStatusItem({ id: "search.index", align: "right", order: -20, component: IndexStatus });

registerSearchProvider({
  id: "find.text",
  tab: "text",
  title: t("tools.find.text"),
  async search(query, { limit }) {
    if (query.trim().length < 2) return [];
    const result = await findApi.search({ query, limit }).catch(() => null);
    const items: SearchItem[] = [];
    for (const f of result?.files ?? []) {
      for (const m of f.matches) {
        items.push({
          id: `${f.path}:${m.line}`,
          label: m.text,
          matches: m.ranges.flatMap(([s, e]) => Array.from({ length: e - s }, (_, k) => s + k)),
          hint: `${f.path.split("/").pop()} ${m.line + 1}`,
          kind: "text",
          // Pre-ranked in file order.
          score: 1_000_000 - items.length,
          run: () => void openFileAt(f.path, m.line + 1, m.column + 1),
        });
      }
    }
    return items;
  },
});

if (typeof document !== "undefined") {
  const host = document.createElement("div");
  host.id = "find-overlay";
  document.body.appendChild(host);
  render(
    () => (
      <Show when={findOpen()}>
        <FindInFilesPopup />
      </Show>
    ),
    host,
  );
}
