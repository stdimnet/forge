/**
 * Recent Locations popup (Ctrl+Shift+E, PHPStorm's `RecentLocations`): recently visited and
 * changed places with a few lines of code each. Pressing the shortcut again toggles "Show
 * changed only", which lists the edit history of Last Edit Location
 * (`features/editorNavigation/editHistory.ts`); typing filters (speed search, ranked in Rust
 * by `fuzzy_filter`).
 */
import { For, Show, createEffect, createMemo, createRoot, createSignal, on, onMount } from "solid-js";
import { render } from "solid-js/web";
import { EditorView } from "@codemirror/view";
import { Search } from "lucide-solid";
import { registerCommand } from "@/state/commands";
import { registerEditorExtension } from "@/editor/extensions";
import { isSyncedTransaction } from "@/editor/docSync";
import { openFileAt } from "@/editor/views";
import { editor, isCustomTab, onPathMoved } from "@/state/editor";
import { workspaceApi } from "@/ipc/workspace";
import { actionById, shortcutLabel } from "@/keymap";
import { FileIcon } from "@/shell/FileIcon";
import { createFuzzyFilter } from "@/features/search/fuzzyFilter";
import { Highlight } from "@/features/search/SearchEverywhere";
import { editPlaces } from "@/features/editorNavigation/editHistory";
import { movePath, recordLocation, shiftLocations, snippet, type Location } from "./locations";
import { t as tr } from "@/i18n";

const [locations, setLocations] = createSignal<Location[]>([]);
export { locations };

/** Lines a caret jump must cover to count as a new place. */
const JUMP = 10;

const isProjectFile = (path: string) => !isCustomTab(path) && !path.includes("://");

registerEditorExtension(({ path }) =>
  isProjectFile(path)
    ? EditorView.updateListener.of((u) => {
        const head = u.state.selection.main.head;
        const line = u.state.doc.lineAt(head).number;
        if (u.docChanged) {
          if (u.transactions.every(isSyncedTransaction) || !u.view.hasFocus) return;
          let list = locations();
          u.changes.iterChanges((fromA, toA, _fromB, _toB, inserted) => {
            const at = u.startState.doc.lineAt(fromA).number;
            const removed = u.startState.doc.lineAt(toA).number - at;
            list = shiftLocations(list, path, at, inserted.lines - 1 - removed);
          });
          setLocations(recordLocation(list, path, line, true));
        } else if (u.selectionSet && u.view.hasFocus) {
          const before = u.startState.doc.lineAt(u.startState.selection.main.head).number;
          if (Math.abs(line - before) >= JUMP) {
            setLocations(recordLocation(recordLocation(locations(), path, before, false), path, line, false));
          }
        }
      })
    : null,
);

createRoot(() => {
  // Switching files records the place in the file that becomes active.
  createEffect(
    on(
      () => editor.active,
      (path) => {
        if (!path || !isProjectFile(path)) return;
        const tab = editor.tabs.find((t) => t.path === path);
        if (tab) setLocations((l) => recordLocation(l, path, tab.cursor.line, false));
      },
    ),
  );
});
onPathMoved((from, to) => setLocations((l) => movePath(l, from, to)));

const [open, setOpen] = createSignal(false);
const [changedOnly, setChangedOnly] = createSignal(false);

export function openRecentLocations(): void {
  if (open()) {
    setChangedOnly((v) => !v);
    return;
  }
  setChangedOnly(false);
  setOpen(true);
}

registerCommand({
  id: "RecentLocations",
  title: actionById("RecentLocations")?.title ?? "Recent Locations",
  category: "View",
  run: openRecentLocations,
});

function RecentLocationsPopup() {
  let root!: HTMLDivElement;
  let list!: HTMLDivElement;
  const [query, setQuery] = createSignal("");
  const [selected, setSelected] = createSignal(0);
  const [texts, setTexts] = createSignal<Record<string, string>>({});

  const shown = createMemo<Location[]>(() =>
    changedOnly() ? editPlaces().map((p, i) => ({ path: p.path, line: p.line + 1, changed: true, seq: -i })) : locations(),
  );

  // Texts of the files: open documents first, the rest from disk.
  createEffect(
    on(shown, async (locs) => {
      const out: Record<string, string> = {};
      for (const path of new Set(locs.map((l) => l.path))) {
        const tab = editor.tabs.find((t) => t.path === path);
        out[path] = tab?.content ?? (await workspaceApi.readFile(path).catch(() => ""));
      }
      setTexts(out);
    }),
  );

  const nameOf = (path: string) => path.split("/").pop() ?? path;
  const lineText = (l: Location) => (texts()[l.path] ?? "").split("\n")[l.line - 1]?.trim() ?? "";
  const hits = createFuzzyFilter(() => ({
    query: query(),
    candidates: shown().map((l) => [nameOf(l.path), lineText(l)]),
  }));
  const rows = createMemo(() => hits().map((h) => ({ loc: shown()[h.index], positions: h.text === 0 ? h.positions : [] })).filter((r) => r.loc));

  createEffect(on(rows, () => setSelected(0)));
  onMount(() => root.focus());

  const go = (loc: Location | undefined) => {
    if (!loc) return;
    setOpen(false);
    void openFileAt(loc.path, loc.line);
  };

  const move = (delta: number) => {
    const n = rows().length;
    if (!n) return;
    setSelected((i) => (i + delta + n) % n);
    list.querySelector(`[data-row="${selected()}"]`)?.scrollIntoView({ block: "nearest" });
  };

  function onKeyDown(e: KeyboardEvent) {
    switch (e.key) {
      case "ArrowDown":
        move(1);
        break;
      case "ArrowUp":
        move(-1);
        break;
      case "Enter":
        go(rows()[selected()]?.loc);
        break;
      case "Escape":
        if (query()) setQuery("");
        else setOpen(false);
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

  const title = () => (changedOnly() ? "Recently Changed Locations" : "Recent Locations");

  return (
    <div class="popup-backdrop" onMouseDown={(e) => e.target === e.currentTarget && setOpen(false)}>
      <div class="popup recent-popup locations-popup" tabIndex={-1} ref={root} role="dialog" aria-label={title()} onKeyDown={onKeyDown}>
        <div class="recent-header">
          <span class="recent-title">{title()}</span>
          <Show when={query()}>
            <span class="speed-search">
              <Search size={12} />
              {query()}
            </span>
          </Show>
          <label class="se-check recent-check" onMouseDown={(e) => e.preventDefault()}>
            <input type="checkbox" checked={changedOnly()} onChange={(e) => setChangedOnly(e.currentTarget.checked)} />
            <span>{tr("tools.editorTabs.showChangedOnly")}</span>
          </label>
        </div>
        <div class="locations-list" ref={list}>
          <Show when={rows().length} fallback={<div class="se-empty">{changedOnly() ? tr("tools.editorTabs.noChangedLocations") : tr("tools.editorTabs.noRecentLocations")}</div>}>
            <For each={rows()}>
              {(row, i) => (
                <div
                  class="location-row"
                  classList={{ selected: selected() === i() }}
                  data-row={i()}
                  onMouseMove={() => setSelected(i())}
                  onClick={() => go(row.loc)}
                >
                  <div class="location-head">
                    <FileIcon name={nameOf(row.loc.path)} size={14} />
                    <span class="recent-name">
                      <Highlight text={nameOf(row.loc.path)} positions={row.positions} />
                    </span>
                    <span class="recent-path">{row.loc.path.split("/").slice(0, -1).join("/")}</span>
                    <Show when={row.loc.changed}>
                      <span class="location-changed" title={tr("tools.editorTabs.changedHere")} />
                    </Show>
                  </div>
                  <div class="location-code">
                    <For each={snippet(texts()[row.loc.path] ?? "", row.loc.line)}>
                      {([n, text]) => (
                        <div class="location-line" classList={{ current: n === row.loc.line }}>
                          <span class="location-gutter">{n}</span>
                          <span class="location-text">{text || " "}</span>
                        </div>
                      )}
                    </For>
                  </div>
                </div>
              )}
            </For>
          </Show>
        </div>
        <div class="se-footer">{tr("tools.editorTabs.pressAgainToShowChanged", { value: shortcutLabel("RecentLocations") })}</div>
      </div>
    </div>
  );
}

if (typeof document !== "undefined") {
  const host = document.createElement("div");
  host.id = "recent-locations";
  document.body.appendChild(host);
  render(
    () => (
      <Show when={open()}>
        <RecentLocationsPopup />
      </Show>
    ),
    host,
  );
}
