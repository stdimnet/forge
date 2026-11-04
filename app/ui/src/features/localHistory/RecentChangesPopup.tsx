/**
 * Recent Changes (Alt+Shift+C): the latest project-wide Local History events (refactorings,
 * external changes, updates from VCS, labels; saves in a row are one entry). Enter opens
 * the change with its files and a diff, where it can be reverted.
 */
import { createEffect, createMemo, createResource, createSignal, For, on, onMount, Show } from "solid-js";
import { Dynamic } from "solid-js/web";
import { Search } from "lucide-solid";
import { localHistoryApi, type LhRecentChange } from "@/ipc/localHistory";
import { createFuzzyFilter } from "@/features/search/fuzzyFilter";
import { shortcutLabel } from "@/keymap";
import { historyDate, historyRevision, nameOf, openHistoryTab, setRecentChangesOpen } from "./actions";
import { KIND_ICONS } from "./LocalHistoryView";
import { t } from "@/i18n";

const summary = (c: LhRecentChange) => {
  if (c.kind === "label") return "Label";
  const names = c.files.slice(0, 3).map(nameOf).join(", ");
  return c.fileCount > 3 ? `${names} and ${c.fileCount - 3} more` : names;
};

export function RecentChangesPopup() {
  let root!: HTMLDivElement;
  const [changes] = createResource(historyRevision, () => localHistoryApi.recent(100));
  const [query, setQuery] = createSignal("");
  const [index, setIndex] = createSignal(0);
  const all = () => changes() ?? [];
  const hits = createFuzzyFilter(() => ({ query: query(), candidates: all().map((c) => `${c.name} ${c.files.map(nameOf).join(" ")}`) }));
  const shown = createMemo(() =>
    hits()
      .map((h) => h.index)
      .sort((a, b) => a - b)
      .flatMap((i) => all()[i] ?? []),
  );
  const close = () => setRecentChangesOpen(false);

  onMount(() => root.focus());
  createEffect(on(query, () => setIndex(0), { defer: true }));
  createEffect(() => root.querySelector(`[data-rc="${index()}"]`)?.scrollIntoView({ block: "nearest" }));

  const open = (c: LhRecentChange | undefined) => {
    close();
    if (c) openHistoryTab({ kind: "change", from: c.from, to: c.to, name: c.name || "Change", time: c.time });
  };

  function onKeyDown(e: KeyboardEvent) {
    const move = (d: number) => setIndex((i) => Math.max(0, Math.min(shown().length - 1, i + d)));
    switch (e.key) {
      case "ArrowDown":
        move(1);
        break;
      case "ArrowUp":
        move(-1);
        break;
      case "Enter":
        open(shown()[index()]);
        break;
      case "Escape":
        if (query()) setQuery("");
        else close();
        break;
      case "Backspace":
        setQuery((q) => q.slice(0, -1));
        break;
      default:
        if (e.key.length === 1 && !e.ctrlKey && !e.metaKey && !e.altKey) setQuery((q) => q + e.key);
        else return;
    }
    e.preventDefault();
  }

  return (
    <div class="popup-backdrop" onMouseDown={(e) => e.target === e.currentTarget && close()}>
      <div class="popup recent-popup recent-changes-popup" tabIndex={-1} ref={root} role="dialog" aria-label={t("tools.localHistory.recentChanges")} onKeyDown={onKeyDown}>
        <div class="recent-header">
          <span class="recent-title">{t("tools.localHistory.recentChanges")}</span>
          <Show when={query()}>
            <span class="speed-search">
              <Search size={12} />
              {query()}
            </span>
          </Show>
        </div>
        <div class="recent-changes-list">
          <Show when={shown().length} fallback={<div class="se-empty">{changes.loading ? t("common.loadingEllipsis") : query() ? t("common.nothingFound") : t("tools.localHistory.noChangesRecordedYet")}</div>}>
            <For each={shown()}>
              {(c, i) => (
                <div
                  class="recent-row"
                  classList={{ selected: index() === i() }}
                  data-rc={i()}
                  title={c.files.join("\n")}
                  onMouseMove={() => setIndex(i())}
                  onClick={() => open(c)}
                >
                  <Dynamic component={KIND_ICONS[c.kind]} size={15} class={`lh-kind lh-kind-${c.kind}`} />
                  <span class="rc-text">
                    <span class="recent-name">{c.name || t("tools.localHistory.change")}</span>
                    <span class="rc-files">{summary(c)}</span>
                  </span>
                  <span class="rc-time">{historyDate(c.time)}</span>
                </div>
              )}
            </For>
          </Show>
        </div>
        <div class="se-footer">
          {t("tools.localHistory.enterToShowTheChange", { value: shortcutLabel("RecentChanges") || "Alt+Shift+C" })}
        </div>
      </div>
    </div>
  );
}
