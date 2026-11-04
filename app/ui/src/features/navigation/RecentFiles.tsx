/**
 * Recent Files popup (Ctrl+E) and Switcher (Ctrl+Tab), laid out like PHPStorm: tool
 * windows on the left, recently opened files on the right, speed search by typing.
 */
import { t, t as tr } from "@/i18n";
import { For, Show, createEffect, createMemo, createSignal, on, onMount } from "solid-js";
import { Dynamic } from "solid-js/web";
import { Search } from "lucide-solid";
import { toolWindows, type ToolWindow } from "@/features/registry";
import { isToolWindowVisible, showToolWindow } from "@/state/layout";
import { editor, isDirty } from "@/state/editor";
import { FileIcon } from "@/shell/FileIcon";
import { shortcutLabel } from "@/keymap";
import { createFuzzyFilter } from "@/features/search/fuzzyFilter";
import { Highlight } from "@/features/search/SearchEverywhere";
import { editedFiles, recentFiles } from "./history";
import { openFileReported } from "./openReported";

export type RecentMode = "recent" | "switcher";

const [recentOpen, setRecentOpen] = createSignal<RecentMode | null>(null);
const [editedOnly, setEditedOnly] = createSignal(false);
export { recentOpen };

/** Open the popup; pressing Ctrl+E again while open toggles "Show edited only". */
export function openRecentFiles(mode: RecentMode = "recent", edited = false): void {
  if (recentOpen() === mode && mode === "recent") {
    setEditedOnly((v) => !v);
    return;
  }
  setEditedOnly(edited);
  setRecentOpen(mode);
}

export const closeRecentFiles = () => setRecentOpen(null);

const TOOL_WINDOW_ACTIONS: Record<string, string> = {
  project: "ActivateProjectToolWindow",
  bookmarks: "ActivateBookmarksToolWindow",
  find: "ActivateFindToolWindow",
  run: "ActivateRunToolWindow",
  debug: "ActivateDebugToolWindow",
  problems: "ActivateProblemsViewToolWindow",
  structure: "ActivateStructureToolWindow",
  services: "ActivateServicesToolWindow",
  git: "ActivateVersionControlToolWindow",
  commit: "ActivateCommitToolWindow",
  terminal: "ActivateTerminalToolWindow",
};

/** PHPStorm shows the tool window number as a mnemonic ("1" for Project). */
const mnemonic = (tw: ToolWindow) => {
  const action = TOOL_WINDOW_ACTIONS[tw.id];
  const key = action ? shortcutLabel(action) : "";
  const m = /(?:Alt\+|⌘)(\d)$/.exec(key);
  return m ? m[1] : "";
};

const rank = (tw: ToolWindow) => {
  const m = mnemonic(tw);
  return m === "" ? 99 : m === "0" ? 10 : Number(m);
};

export function RecentFilesPopup() {
  let root!: HTMLDivElement;
  const [query, setQuery] = createSignal("");
  const [column, setColumn] = createSignal<"files" | "windows">("files");
  const [fileIdx, setFileIdx] = createSignal(0);
  const [twIdx, setTwIdx] = createSignal(0);

  const allFiles = createMemo(() => {
    const source = editedOnly() ? editedFiles() : recentFiles();
    // Open tabs that were never activated still count as recent (e.g. restored session).
    return [...source, ...(editedOnly() ? [] : editor.tabs.map((t) => t.path).filter((p) => !source.includes(p)))];
  });
  const nameOf = (path: string) => path.split("/").pop() ?? path;
  // Matched and ranked in Rust; ties keep the recency order.
  const fileHits = createFuzzyFilter(() => ({ query: query(), candidates: allFiles().map(nameOf) }));
  const files = createMemo(() =>
    fileHits()
      .filter((h) => h.index < allFiles().length)
      .map((h) => {
        const path = allFiles()[h.index];
        return { path, name: nameOf(path), m: h };
      }),
  );

  const sortedWindows = createMemo(() =>
    // Numbered windows first in keyboard order (1…9, 0), then the rest alphabetically.
    [...toolWindows()].sort((a, b) => rank(a) - rank(b) || a.title.localeCompare(b.title)),
  );
  const windowHits = createFuzzyFilter(() => ({ query: query(), candidates: sortedWindows().map((tw) => tw.title) }));
  const windows = createMemo(() =>
    windowHits()
      .map((h) => h.index)
      .sort((a, b) => a - b)
      .flatMap((i) => sortedWindows()[i] ?? []),
  );

  onMount(() => {
    root.focus();
    // Switcher (and Ctrl+E from an open file) preselects the previous file, like PHPStorm.
    if (files().length > 1 && files()[0].path === editor.active) setFileIdx(1);
  });

  createEffect(
    on(
      query,
      () => {
        setFileIdx(0);
        setTwIdx(0);
      },
      { defer: true },
    ),
  );

  createEffect(() => {
    const sel = column() === "files" ? `[data-file="${fileIdx()}"]` : `[data-tw="${twIdx()}"]`;
    root.querySelector(sel)?.scrollIntoView({ block: "nearest" });
  });

  async function openSelected() {
    if (column() === "files") {
      const f = files()[fileIdx()];
      closeRecentFiles();
      if (f) await openFileReported(f.path);
    } else {
      const tw = windows()[twIdx()];
      closeRecentFiles();
      if (tw) showToolWindow(tw.id);
    }
  }

  function onKeyDown(e: KeyboardEvent) {
    const move = (d: number) => {
      if (column() === "files") setFileIdx((i) => Math.max(0, Math.min(files().length - 1, i + d)));
      else setTwIdx((i) => Math.max(0, Math.min(windows().length - 1, i + d)));
    };
    switch (e.key) {
      case "ArrowDown":
        move(1);
        break;
      case "ArrowUp":
        move(-1);
        break;
      case "ArrowLeft":
        setColumn("windows");
        break;
      case "ArrowRight":
        setColumn("files");
        break;
      case "Enter":
        void openSelected();
        break;
      case "Escape":
        if (query()) setQuery("");
        else closeRecentFiles();
        break;
      case "Backspace":
        setQuery((q) => q.slice(0, -1));
        break;
      case "Tab":
        // Switcher: Tab cycles forward, Shift+Tab backwards.
        move(e.shiftKey ? -1 : 1);
        break;
      default:
        if (e.key.length === 1 && !e.ctrlKey && !e.metaKey && !e.altKey) setQuery((q) => q + e.key);
        else return;
    }
    e.preventDefault();
  }

  function onKeyUp(e: KeyboardEvent) {
    // Switcher opens the selection when Ctrl is released.
    if (recentOpen() === "switcher" && e.key === "Control") void openSelected();
  }

  const title = () => (recentOpen() === "switcher" ? "Switcher" : editedOnly() ? "Recently Edited Files" : "Recent Files");

  return (
    <div class="popup-backdrop" onMouseDown={(e) => e.target === e.currentTarget && closeRecentFiles()}>
      <div
        class="popup recent-popup"
        tabIndex={-1}
        ref={root}
        role="dialog"
        aria-label={title()}
        aria-activedescendant={column() === "windows" ? `recent-tw-${twIdx()}` : files().length ? `recent-file-${fileIdx()}` : undefined}
        onKeyDown={onKeyDown}
        onKeyUp={onKeyUp}
      >
        <div class="recent-header">
          <span class="recent-title">{title()}</span>
          <Show when={query()}>
            <span class="speed-search">
              <Search size={12} />
              {query()}
            </span>
          </Show>
          <label class="se-check recent-check" onMouseDown={(e) => e.preventDefault()}>
            <input type="checkbox" checked={editedOnly()} onChange={(e) => setEditedOnly(e.currentTarget.checked)} />
            <span>{tr("tools.navigation.showEditedOnly")}</span>
          </label>
        </div>
        <div class="recent-body">
          <div class="recent-windows" role="listbox" aria-label={t("a11y.recent.toolWindows")}>
            <For each={windows()}>
              {(tw, i) => (
                <div
                  class="recent-row"
                  classList={{ selected: column() === "windows" && twIdx() === i(), active: isToolWindowVisible(tw.id) }}
                  data-tw={i()}
                  id={`recent-tw-${i()}`}
                  role="option"
                  aria-selected={column() === "windows" && twIdx() === i()}
                  onMouseMove={() => {
                    setColumn("windows");
                    setTwIdx(i());
                  }}
                  onClick={() => void openSelected()}
                >
                  <Dynamic component={tw.icon} size={15} stroke-width={1.75} />
                  <span class="recent-name">{tw.title}</span>
                  <span class="recent-mnemonic">{mnemonic(tw)}</span>
                </div>
              )}
            </For>
          </div>
          <div class="recent-files" role={files().length ? "listbox" : undefined} aria-label={t("a11y.recent.files")}>
            <Show when={files().length} fallback={<div class="se-empty">{editedOnly() ? tr("tools.navigation.noEditedFiles") : tr("tools.navigation.noRecentFiles")}</div>}>
              <For each={files()}>
                {(f, i) => {
                  const tab = () => editor.tabs.find((t) => t.path === f.path);
                  return (
                    <div
                      class="recent-row"
                      classList={{ selected: column() === "files" && fileIdx() === i() }}
                      data-file={i()}
                      id={`recent-file-${i()}`}
                      role="option"
                      aria-selected={column() === "files" && fileIdx() === i()}
                      onMouseMove={() => {
                        setColumn("files");
                        setFileIdx(i());
                      }}
                      onClick={() => void openSelected()}
                    >
                      <FileIcon name={f.name} size={15} />
                      <span class="recent-name" classList={{ modified: !!tab() && isDirty(tab()!) }}>
                        <Highlight text={f.name} positions={f.m!.positions} />
                      </span>
                      <span class="recent-path">{f.path.split("/").slice(0, -1).join("/")}</span>
                    </div>
                  );
                }}
              </For>
            </Show>
          </div>
        </div>
        <div class="se-footer">
          {recentOpen() === "switcher"
            ? tr("tools.navigation.releaseCtrlToOpenTab")
            : tr("tools.navigation.pressAgainToShowEdited", { value: shortcutLabel("RecentFiles") })}
        </div>
      </div>
    </div>
  );
}
