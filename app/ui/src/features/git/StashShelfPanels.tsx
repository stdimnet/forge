/**
 * The Commit tool window's tabs, like PHPStorm: **Commit**, **Stash** (the git stashes with
 * Apply, Pop, Drop and Show Diff) and **Shelf** (shelved changelists with Unshelve, Unshelve
 * Silently, Delete, Show Diff and Rename). Stash and Shelf only appear while they have entries.
 */
import { createEffect, createRoot, createSignal, For, on, Show, type Component } from "solid-js";
import { ArchiveRestore, ChevronDown, ChevronRight, FileDiff, Pencil, RefreshCw, Trash2, Undo2, Upload } from "lucide-solid";
import { FileIcon } from "@/shell/FileIcon";
import { askConfirm, askName } from "@/features/contextMenu/dialogs";
import { gitApi, type ChangedFile, type StashEntry } from "./api";
import { opsApi, type ShelvedChangelist } from "./opsApi";
import { repoAvailable, revision, runGit, notify } from "./store";
import { logDate, shortDate, timeAgo } from "./format";
import { CommitPanel } from "./CommitPanel";
import { applyStash, dropStash, showShelfDiff, showStashDiff, unshelve, unshelveDialog } from "./gitDialogs";
import { t } from "@/i18n";

export type CommitTab = "commit" | "stash" | "shelf";
const [commitTab, setCommitTab] = createSignal<CommitTab>("commit");
export { commitTab, setCommitTab };

const [stashes, setStashes] = createSignal<StashEntry[]>([]);
const [shelf, setShelf] = createSignal<ShelvedChangelist[]>([]);
export { stashes, shelf };

export async function refreshStashAndShelf(): Promise<void> {
  if (!repoAvailable()) {
    setStashes([]);
    setShelf([]);
    return;
  }
  const [s, sh] = await Promise.all([gitApi.stashList().catch(() => []), opsApi.shelfList().catch(() => [])]);
  setStashes(s);
  setShelf(sh);
  setStashFiles(new Map());
}

createRoot(() => createEffect(on(revision, () => void refreshStashAndShelf())));

const nameOf = (p: string) => p.slice(p.lastIndexOf("/") + 1);
const dirOf = (p: string) => (p.includes("/") ? p.slice(0, p.lastIndexOf("/")) : "");
const statusClass = (s: string) => (s === "A" || s === "added" ? "added" : s === "D" || s === "deleted" ? "deleted" : "modified");

const tabs = (): { id: CommitTab; title: string }[] => [
  { id: "commit", title: t("vcs.tab.changes") },
  ...(shelf().length || commitTab() === "shelf" ? [{ id: "shelf" as const, title: t("vcs.tab.shelf") }] : []),
  ...(stashes().length || commitTab() === "stash" ? [{ id: "stash" as const, title: t("vcs.tab.stash") }] : []),
];

/** Header tabs of the Commit tool window. */
export function CommitTabs() {
  return (
    <Show when={tabs().length > 1}>
      <div class="run-tabs" role="tablist">
        <For each={tabs()}>
          {(t) => (
            <div class="run-tab" classList={{ active: commitTab() === t.id }} role="tab" data-commit-tab={t.id} onClick={() => setCommitTab(t.id)}>
              <span class="run-tab-title">{t.title}</span>
            </div>
          )}
        </For>
      </div>
    </Show>
  );
}

/** The Commit tool window: the commit panel or the Stash / Shelf tab. */
export const CommitToolWindow: Component = () => (
  <Show when={commitTab() !== "commit" && tabs().some((t) => t.id === commitTab())} fallback={<CommitPanel />}>
    <Show when={commitTab() === "stash"} fallback={<ShelfPanel />}>
      <StashPanel />
    </Show>
  </Show>
);

function Chevron(props: { open: boolean }) {
  return <span class="tree-chevron">{props.open ? <ChevronDown size={14} /> : <ChevronRight size={14} />}</span>;
}

function FileRow(props: { path: string; status: string; selected: boolean; onClick: (e: MouseEvent) => void; onOpen: () => void }) {
  return (
    <div
      class="tree-row commit-row"
      classList={{ selected: props.selected }}
      style={{ "padding-left": "26px" }}
      data-path={props.path}
      onClick={(e) => props.onClick(e)}
      onDblClick={() => props.onOpen()}
      title={props.path}
    >
      <span class="tree-chevron" />
      <FileIcon name={nameOf(props.path)} />
      <span class={`tree-label vcs-${statusClass(props.status)}`}>{nameOf(props.path)}</span>
      <Show when={dirOf(props.path)}>
        <span class="tree-hint">{dirOf(props.path)}</span>
      </Show>
    </div>
  );
}

// ---- Stash ---------------------------------------------------------------------------------

const [stashFiles, setStashFiles] = createSignal<Map<number, ChangedFile[]>>(new Map());
const [stashOpen, setStashOpen] = createSignal<Set<number>>(new Set());
const [stashSel, setStashSel] = createSignal<{ index: number; path?: string } | null>(null);

async function toggleStash(index: number): Promise<void> {
  const open = new Set(stashOpen());
  if (open.has(index)) open.delete(index);
  else {
    open.add(index);
    if (!stashFiles().has(index)) {
      const files = (await opsApi.stashFiles(index).catch(() => [])) ?? [];
      setStashFiles((m) => new Map(m).set(index, files));
    }
  }
  setStashOpen(open);
}

const currentStash = () => stashes().find((s) => s.index === stashSel()?.index) ?? null;

/** Show Diff (Ctrl+D) in the Stash tab: the selected file, else the stash's first file. */
export async function diffSelectedStash(): Promise<void> {
  const entry = currentStash();
  if (!entry) return;
  const path = stashSel()?.path ?? (await opsApi.stashFiles(entry.index).catch(() => []))[0]?.path;
  if (path) await showStashDiff(entry, path);
}

function StashPanel() {
  const current = currentStash;
  const diff = () => void diffSelectedStash();
  const run = async (fn: (e: StashEntry) => Promise<boolean>) => {
    const entry = current();
    if (entry && (await fn(entry))) setStashSel(null);
  };
  return (
    <div class="vcs-tab-panel stash-panel" tabIndex={-1}>
      <div class="panel-toolbar">
        <button class="icon-btn" title={t("git.stashShelfPanels.refresh")} onClick={() => void refreshStashAndShelf()}>
          <RefreshCw size={15} />
        </button>
        <div class="toolbar-sep" />
        <button class="icon-btn" title={t("common.apply")} disabled={!current()} onClick={() => void run((e) => applyStash(e, { pop: false, reinstateIndex: false }))}>
          <ArchiveRestore size={15} />
        </button>
        <button class="icon-btn" title={t("git.stashShelfPanels.pop")} disabled={!current()} onClick={() => void run((e) => applyStash(e, { pop: true, reinstateIndex: false }))}>
          <Upload size={15} />
        </button>
        <button class="icon-btn" title={t("git.stashShelfPanels.drop")} disabled={!current()} onClick={() => void run(dropStash)}>
          <Trash2 size={15} />
        </button>
        <div class="toolbar-sep" />
        <button class="icon-btn" title={t("git.stashShelfPanels.showDiff", { shortcut: "Ctrl+D" })} disabled={!current()} onClick={diff}>
          <FileDiff size={15} />
        </button>
      </div>
      <div class="vcs-tab-tree commit-tree" role="tree">
        <For each={stashes()} fallback={<div class="vcs-tab-empty">{t("git.stashShelfPanels.noStashes")}</div>}>
          {(s) => (
            <>
              <div
                class="tree-row commit-group vcs-tab-node"
                classList={{ selected: stashSel()?.index === s.index && !stashSel()?.path }}
                style={{ "padding-left": "8px" }}
                data-stash={s.index}
                onClick={() => setStashSel({ index: s.index })}
                onDblClick={() => void toggleStash(s.index)}
                title={`stash@{${s.index}}`}
              >
                <span onClick={(e) => (e.stopPropagation(), void toggleStash(s.index))}>
                  <Chevron open={stashOpen().has(s.index)} />
                </span>
                <span class="tree-label">{s.message}</span>
                <span class="tree-hint">
                  {s.branch ? `${s.branch} · ` : ""}
                  {timeAgo(s.time)}
                </span>
              </div>
              <Show when={stashOpen().has(s.index)}>
                <For each={stashFiles().get(s.index) ?? []}>
                  {(f) => (
                    <FileRow
                      path={f.path}
                      status={f.status}
                      selected={stashSel()?.index === s.index && stashSel()?.path === f.path}
                      onClick={() => setStashSel({ index: s.index, path: f.path })}
                      onOpen={() => void showStashDiff(s, f.path)}
                    />
                  )}
                </For>
              </Show>
            </>
          )}
        </For>
      </div>
    </div>
  );
}

// ---- Shelf ---------------------------------------------------------------------------------

const [shelfOpen, setShelfOpen] = createSignal<Set<string>>(new Set());
const [shelfSel, setShelfSel] = createSignal<{ id: string; paths: string[] } | null>(null);

function toggleShelf(id: string): void {
  const open = new Set(shelfOpen());
  open.has(id) ? open.delete(id) : open.add(id);
  setShelfOpen(open);
}

/** Expand a changelist and select it (Show Shelf, screenshots). */
export function revealShelved(id: string, paths: string[] = []): void {
  setShelfOpen((s) => new Set(s).add(id));
  setShelfSel({ id, paths });
}

const currentShelved = () => shelf().find((c) => c.id === shelfSel()?.id) ?? null;
const selPaths = () => (shelfSel()?.paths.length ? shelfSel()!.paths : null);

/** Show Diff (Ctrl+D) in the Shelf tab: the selected file, else the changelist's first file. */
export async function diffSelectedShelved(): Promise<void> {
  const cl = currentShelved();
  const path = cl && (selPaths()?.[0] ?? cl.files[0]?.path);
  if (cl && path) await showShelfDiff(cl, path);
}

/** Unshelve… (ShelveChanges.UnshelveWithDialog): the selected changelist or files. */
export function unshelveSelected(): void {
  const cl = currentShelved() ?? shelf()[0];
  if (!cl) return notify("info", t("git.stashShelfPanels.noShelvedChanges"));
  unshelveDialog(cl, cl.id === shelfSel()?.id ? (selPaths() ?? undefined) : undefined);
}

function ShelfPanel() {
  const current = currentShelved;
  const pickFile = (cl: ShelvedChangelist, path: string, e: MouseEvent) => {
    const sel = shelfSel();
    if ((e.ctrlKey || e.metaKey) && sel?.id === cl.id) {
      const paths = sel.paths.includes(path) ? sel.paths.filter((p) => p !== path) : [...sel.paths, path];
      setShelfSel({ id: cl.id, paths });
    } else setShelfSel({ id: cl.id, paths: [path] });
  };
  const silently = async () => {
    const cl = current();
    if (cl && (await unshelve(cl, selPaths(), true))) setShelfSel(null);
  };
  const remove = async () => {
    const cl = current();
    if (!cl) return;
    const ok = await askConfirm({ title: t("git.stashShelfPanels.deleteShelvedChanges"), message: t("git.stashShelfPanels.deleteTheShelvedChangelistName", { name: cl.name }), confirm: t("git.stashShelfPanels.delete") });
    if (!ok) return;
    const done = await runGit("Could not delete the shelved changes", () => opsApi.shelfDelete(cl.id).then(() => true));
    if (done) setShelfSel(null);
  };
  const rename = async () => {
    const cl = current();
    if (!cl) return;
    const name = await askName({ title: t("git.stashShelfPanels.renameShelvedChangelist"), label: t("git.stashShelfPanels.name"), initial: cl.name, confirm: t("git.stashShelfPanels.rename") });
    if (name?.trim()) await runGit("Could not rename the shelved changes", () => opsApi.shelfRename(cl.id, name.trim()));
  };
  const diff = () => void diffSelectedShelved();
  return (
    <div class="vcs-tab-panel shelf-panel" tabIndex={-1}>
      <div class="panel-toolbar">
        <button class="icon-btn" title={t("git.stashShelfPanels.refresh")} onClick={() => void refreshStashAndShelf()}>
          <RefreshCw size={15} />
        </button>
        <div class="toolbar-sep" />
        <button class="icon-btn" title={t("git.stashShelfPanels.unshelve")} disabled={!current()} onClick={() => current() && unshelveDialog(current()!, selPaths() ?? undefined)}>
          <ArchiveRestore size={15} />
        </button>
        <button class="icon-btn" title={t("git.stashShelfPanels.unshelveSilently")} disabled={!current()} onClick={() => void silently()}>
          <Undo2 size={15} />
        </button>
        <button class="icon-btn" title={t("git.stashShelfPanels.delete")} disabled={!current()} onClick={() => void remove()}>
          <Trash2 size={15} />
        </button>
        <div class="toolbar-sep" />
        <button class="icon-btn" title={t("git.stashShelfPanels.showDiff", { shortcut: "Ctrl+D" })} disabled={!current()} onClick={diff}>
          <FileDiff size={15} />
        </button>
        <button class="icon-btn" title={t("git.stashShelfPanels.rename")} disabled={!current()} onClick={() => void rename()}>
          <Pencil size={14} />
        </button>
      </div>
      <div class="vcs-tab-tree commit-tree" role="tree">
        <For each={shelf()} fallback={<div class="vcs-tab-empty">{t("git.stashShelfPanels.noShelvedChanges")}</div>}>
          {(cl) => (
            <>
              <div
                class="tree-row commit-group vcs-tab-node"
                classList={{ selected: shelfSel()?.id === cl.id && !shelfSel()?.paths.length }}
                style={{ "padding-left": "8px" }}
                data-shelf={cl.id}
                onClick={() => setShelfSel({ id: cl.id, paths: [] })}
                onDblClick={() => toggleShelf(cl.id)}
                title={`${cl.name} (${logDate(cl.date)})`}
              >
                <span onClick={(e) => (e.stopPropagation(), toggleShelf(cl.id))}>
                  <Chevron open={shelfOpen().has(cl.id)} />
                </span>
                <span class="tree-label">{cl.name}</span>
                <span class="tree-hint">{shortDate(cl.date)}</span>
              </div>
              <Show when={shelfOpen().has(cl.id)}>
                <For each={cl.files}>
                  {(f) => (
                    <FileRow
                      path={f.path}
                      status={f.status}
                      selected={shelfSel()?.id === cl.id && !!shelfSel()?.paths.includes(f.path)}
                      onClick={(e) => pickFile(cl, f.path, e)}
                      onOpen={() => void showShelfDiff(cl, f.path)}
                    />
                  )}
                </For>
              </Show>
            </>
          )}
        </For>
      </div>
    </div>
  );
}

/** Show Shelf (Vcs.Show.Shelf). */
export async function showShelfTab(open: (id: string) => void): Promise<void> {
  await refreshStashAndShelf();
  if (!shelf().length) return notify("info", t("git.stashShelfPanels.noShelvedChanges"));
  setCommitTab("shelf");
  open("commit");
}
