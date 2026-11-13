/**
 * Bookmarks tool window (Alt+2): bookmark lists, the files in each list and their line
 * bookmarks with mnemonic, description and the line's text. Double click (or Enter) jumps
 * to the line; the context menu edits descriptions, moves bookmarks between lists and
 * manages the lists.
 */
import { For, Show, createEffect, createMemo, createSignal, on } from "solid-js";
import { Bookmark as BookmarkIcon, ChevronDown, ChevronRight, FolderPlus, Pencil, Star, Trash2 } from "lucide-solid";
import { FileIcon } from "@/shell/FileIcon";
import { openFileAt } from "@/editor/views";
import { editor } from "@/state/editor";
import { workspaceApi } from "@/ipc/workspace";
import type { Bookmark } from "@/ipc/workbench";
import { askConfirm, askName } from "@/features/contextMenu/dialogs";
import { item, separator, showContextMenu, submenu, type MenuEntry } from "@/features/contextMenu/state";
import { applyBookmarks, bookmarks } from "./store";
import { t as tr } from "@/i18n";

const nameOf = (p: string) => p.slice(p.lastIndexOf("/") + 1);
const dirOf = (p: string) => (p.includes("/") ? p.slice(0, p.lastIndexOf("/")) : "");

const [collapsed, setCollapsed] = createSignal<Set<string>>(new Set());
const [selected, setSelected] = createSignal<string | null>(null);
const toggle = (key: string) =>
  setCollapsed((s) => {
    const n = new Set(s);
    n.has(key) ? n.delete(key) : n.add(key);
    return n;
  });

/** Texts of bookmarked files (open documents first) for the line previews. */
const [texts, setTexts] = createSignal<Record<string, string>>({});

export async function editDescription(b: Bookmark): Promise<void> {
  const description = await askName({
    title: tr("tools.bookmarks.editBookmarkDescription"),
    label: tr("common.description"),
    initial: b.description ?? "",
    confirm: "OK",
    validate: () => null,
  });
  if (description !== null) await applyBookmarks([{ op: "describe", id: b.id, description }]);
}

async function newList(): Promise<void> {
  const name = await askName({ title: tr("tools.bookmarks.newBookmarkList"), label: tr("common.name"), initial: "", confirm: tr("common.create"), validate: (n) => (n.trim() ? null : "Enter a name") });
  if (name) await applyBookmarks([{ op: "addList", name }]);
}

async function renameList(from: string): Promise<void> {
  const to = await askName({
    title: tr("tools.bookmarks.renameBookmarkList"),
    label: tr("common.name"),
    initial: from,
    confirm: tr("tools.bookmarks.rename"),
    validate: (n) => (!n.trim() ? "Enter a name" : n !== from && bookmarks().lists.some((l) => l.name === n) ? "A list with this name exists" : null),
  });
  if (to && to !== from) await applyBookmarks([{ op: "renameList", from, to }]);
}

async function deleteList(name: string): Promise<void> {
  const count = bookmarks().lists.find((l) => l.name === name)?.bookmarks.length ?? 0;
  if (count && !(await askConfirm({ title: tr("tools.bookmarks.deleteBookmarkList"), message: tr("tools.bookmarks.deleteAndItsBookmarks", { count, name }), confirm: tr("common.delete") }))) return;
  await applyBookmarks([{ op: "removeList", name }]);
}

function bookmarkMenu(b: Bookmark & { list: string }): MenuEntry[] {
  const others = bookmarks().lists.filter((l) => l.name !== b.list);
  return [
    item("Jump to Source", () => void openFileAt(b.path, b.line + 1), "EditSource"),
    item("Edit Description…", () => void editDescription(b), "EditBookmark"),
    submenu("Move to Bookmark List", [
      ...others.map((l) => item(l.name, () => void applyBookmarks([{ op: "moveToList", id: b.id, list: l.name }]))),
      ...(others.length ? [separator] : []),
      item("New List…", async () => {
        const name = await askName({ title: tr("tools.bookmarks.newBookmarkList"), label: tr("common.name"), initial: "", confirm: tr("tools.bookmarks.move"), validate: (n) => (n.trim() ? null : "Enter a name") });
        if (name) await applyBookmarks([{ op: "moveToList", id: b.id, list: name }]);
      }),
    ]),
    separator,
    item("Delete", () => void applyBookmarks([{ op: "remove", id: b.id }]), "$Delete"),
  ];
}

function listMenu(name: string): MenuEntry[] {
  const isDefault = bookmarks().defaultList === name;
  return [
    item("New List…", () => void newList()),
    item("Rename List…", () => void renameList(name)),
    ...(isDefault ? [] : [item("Make Default", () => void applyBookmarks([{ op: "setDefaultList", name }]))]),
    separator,
    item("Clear List", () => void applyBookmarks([{ op: "clear", list: name }])),
    ...(bookmarks().lists.length > 1 ? [item("Delete List…", () => void deleteList(name))] : []),
  ];
}

export function BookmarksView() {
  const lists = createMemo(() =>
    bookmarks().lists.map((l) => {
      const files = new Map<string, Bookmark[]>();
      for (const b of [...l.bookmarks].sort((a, b) => a.path.localeCompare(b.path) || a.line - b.line)) {
        files.set(b.path, [...(files.get(b.path) ?? []), b]);
      }
      return { name: l.name, files: [...files.entries()], count: l.bookmarks.length };
    }),
  );

  createEffect(
    on(
      () => [...new Set(bookmarks().lists.flatMap((l) => l.bookmarks.map((b) => b.path)))].join("\n"),
      async (joined) => {
        const out: Record<string, string> = {};
        for (const path of joined ? joined.split("\n") : []) {
          out[path] = editor.tabs.find((t) => t.path === path)?.content ?? (await workspaceApi.readFile(path).catch(() => ""));
        }
        setTexts(out);
      },
    ),
  );
  const lineText = (b: Bookmark) => {
    const doc = editor.tabs.find((t) => t.path === b.path)?.content ?? texts()[b.path] ?? "";
    return doc.split("\n")[b.line]?.trim() ?? "";
  };

  return (
    <div class="bookmarks-view tree-view" tabIndex={0}>
      <For each={lists()}>
        {(list) => {
          const key = `list:${list.name}`;
          const open = () => !collapsed().has(key);
          return (
            <>
              <div
                class="tree-row bookmarks-list-row"
                classList={{ selected: selected() === key }}
                style={{ "padding-left": "6px" }}
                onClick={() => (setSelected(key), toggle(key))}
                onContextMenu={(e) => {
                  e.preventDefault();
                  setSelected(key);
                  showContextMenu(listMenu(list.name), e.clientX, e.clientY);
                }}
              >
                {open() ? <ChevronDown size={14} class="muted" /> : <ChevronRight size={14} class="muted" />}
                <Star size={14} class="bookmarks-list-icon" classList={{ default: bookmarks().defaultList === list.name }} />
                <span class="tree-label strong">{list.name}</span>
                <span class="tree-hint">{list.count ? `${list.count}` : "empty"}</span>
              </div>
              <Show when={open()}>
                <For each={list.files}>
                  {([path, items]) => {
                    const fileKey = `${key}/${path}`;
                    const fileOpen = () => !collapsed().has(fileKey);
                    return (
                      <>
                        <div class="tree-row" classList={{ selected: selected() === fileKey }} style={{ "padding-left": "24px" }} onClick={() => (setSelected(fileKey), toggle(fileKey))}>
                          {fileOpen() ? <ChevronDown size={14} class="muted" /> : <ChevronRight size={14} class="muted" />}
                          <FileIcon name={nameOf(path)} size={14} />
                          <span class="tree-label">{nameOf(path)}</span>
                          <span class="tree-hint">{dirOf(path)}</span>
                        </div>
                        <Show when={fileOpen()}>
                          <For each={items}>
                            {(b) => {
                              const bKey = `bm:${b.id}`;
                              return (
                                <div
                                  class="tree-row bookmark-row"
                                  classList={{ selected: selected() === bKey }}
                                  style={{ "padding-left": "58px" }}
                                  title={`${path}:${b.line + 1}`}
                                  onClick={() => setSelected(bKey)}
                                  onDblClick={() => void openFileAt(b.path, b.line + 1)}
                                  onKeyDown={(e) => e.key === "Enter" && void openFileAt(b.path, b.line + 1)}
                                  onContextMenu={(e) => {
                                    e.preventDefault();
                                    setSelected(bKey);
                                    showContextMenu(bookmarkMenu({ ...b, list: list.name }), e.clientX, e.clientY);
                                  }}
                                >
                                  <Show when={b.mnemonic} fallback={<BookmarkIcon size={13} class="bookmark-icon" />}>
                                    <span class="bookmark-mnemonic">{b.mnemonic}</span>
                                  </Show>
                                  <span class="bookmark-line">{b.line + 1}:</span>
                                  <Show when={b.description}>
                                    <span class="tree-label strong">{b.description}</span>
                                  </Show>
                                  <span class="tree-hint bookmark-text">{lineText(b)}</span>
                                </div>
                              );
                            }}
                          </For>
                        </Show>
                      </>
                    );
                  }}
                </For>
              </Show>
            </>
          );
        }}
      </For>
      <Show when={!bookmarks().lists.some((l) => l.bookmarks.length)}>
        <div class="bookmarks-empty muted">{tr("tools.bookmarks.noBookmarksYetPressF11")}</div>
      </Show>
    </div>
  );
}

/** Header buttons: new list, edit description and delete of the selected bookmark. */
export function BookmarksActions() {
  const selectedBookmark = () => {
    const s = selected();
    if (!s?.startsWith("bm:")) return undefined;
    const id = Number(s.slice(3));
    for (const l of bookmarks().lists) {
      const b = l.bookmarks.find((x) => x.id === id);
      if (b) return b;
    }
    return undefined;
  };
  return (
    <>
      <button class="icon-btn" title={tr("tools.bookmarks.newListEllipsis")} onClick={() => void newList()}>
        <FolderPlus size={15} />
      </button>
      <button class="icon-btn" title={tr("tools.bookmarks.editDescriptionEllipsis")} disabled={!selectedBookmark()} onClick={() => selectedBookmark() && void editDescription(selectedBookmark()!)}>
        <Pencil size={15} />
      </button>
      <button
        class="icon-btn"
        title={tr("common.delete")}
        disabled={!selectedBookmark()}
        onClick={() => selectedBookmark() && void applyBookmarks([{ op: "remove", id: selectedBookmark()!.id }])}
      >
        <Trash2 size={15} />
      </button>
    </>
  );
}
