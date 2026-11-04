/**
 * Bookmarks (PHPStorm: Navigate | Bookmarks, Alt+2): line bookmarks (F11), mnemonic
 * bookmarks (Ctrl+F11, Ctrl+Shift+<digit>, jump with Ctrl+<digit>), bookmark lists, gutter
 * icons and the Bookmarks tool window. Stored per project in Forge's state dir by the
 * backend (`forge-bookmarks`); line positions follow edits.
 */
import { render } from "solid-js/web";
import { Bookmark } from "lucide-solid";
import { registerToolWindow } from "@/features/registry";
import { registerCommand } from "@/state/commands";
import { activeTab } from "@/state/editor";
import { showToolWindow } from "@/state/layout";
import { activeView, openFileAt } from "@/editor/views";
import { actionById, registerKeymapActions, showBalloon } from "@/keymap";
import { registerMainMenuEntries } from "@/features/mainMenu";
import { action as menuAction } from "@/features/mainMenu/model";
import { BookmarksActions, BookmarksView, editDescription } from "./BookmarksView";
import { MnemonicHost, askMnemonic } from "./MnemonicPopup";
import { applyBookmarks, bookmarkAt, bookmarkByMnemonic, bookmarksIn, flushEdits } from "./store";
import "./gutter";
import "./bookmarks.css";
import { t } from "@/i18n";

registerToolWindow({
  id: "bookmarks",
  title: t("toolwindow.bookmarks.title"),
  icon: Bookmark,
  anchor: "left",
  order: 40,
  shortcut: "Alt+2",
  component: BookmarksView,
  actions: () => <BookmarksActions />,
});

registerKeymapActions([
  { id: "GotoNextBookmark", title: "Next Bookmark", group: "Main Menu/Navigate/Bookmarks" },
  { id: "GotoPreviousBookmark", title: "Previous Bookmark", group: "Main Menu/Navigate/Bookmarks" },
  { id: "EditBookmark", title: "Edit Bookmark Description…", group: "Main Menu/Navigate/Bookmarks" },
]);
registerMainMenuEntries("Navigate/Bookmarks", [menuAction("EditBookmark"), menuAction("GotoNextBookmark"), menuAction("GotoPreviousBookmark")], {
  after: "ShowBookmarks",
});

function action(id: string, run: () => void | Promise<void>): void {
  registerCommand({ id, title: actionById(id)?.title ?? id, category: "Bookmarks", run });
}

/** The file and 0-based caret line of the active editor. */
function caret(): { path: string; line: number } | undefined {
  const tab = activeTab();
  const view = activeView();
  if (!tab || !view || tab.view) return undefined;
  return { path: tab.path, line: view.state.doc.lineAt(view.state.selection.main.head).number - 1 };
}

/** Send pending edits first so the caret line and the stored lines agree. */
async function withCaret(fn: (c: { path: string; line: number }) => Promise<void> | void): Promise<void> {
  const c = caret();
  if (!c) return;
  await flushEdits();
  await fn(c);
}

action("ToggleBookmark", () => withCaret(({ path, line }) => applyBookmarks([{ op: "toggle", path, line }])));

action("ToggleBookmarkWithMnemonic", () =>
  withCaret(async ({ path, line }) => {
    const current = bookmarkAt(path, line)?.mnemonic;
    const choice = await askMnemonic(path, line, current);
    activeView()?.focus();
    if (choice === null) return;
    if (choice === "delete") {
      const b = bookmarkAt(path, line);
      if (b) await applyBookmarks([{ op: "remove", id: b.id }]);
      return;
    }
    await applyBookmarks([{ op: "setMnemonic", path, line, mnemonic: choice }]);
  }),
);

action("EditBookmark", () =>
  withCaret(async ({ path, line }) => {
    const b = bookmarkAt(path, line);
    if (!b) return void showBalloon("No bookmark on this line", "Press F11 to add one.");
    await editDescription(b);
  }),
);

action("ShowBookmarks", () => {
  showToolWindow("bookmarks");
  queueMicrotask(() => document.querySelector<HTMLElement>(".bookmarks-view")?.focus());
});

for (let d = 0; d <= 9; d++) {
  const m = String(d);
  action(`ToggleBookmark${m}`, () => withCaret(({ path, line }) => applyBookmarks([{ op: "setMnemonic", path, line, mnemonic: m }])));
  action(`GotoBookmark${m}`, async () => {
    await flushEdits();
    const b = bookmarkByMnemonic(m);
    if (!b) return void showBalloon(`Bookmark ${m} is not set`, "Set it with Ctrl+Shift+" + m + " or Ctrl+F11.");
    await openFileAt(b.path, b.line + 1);
  });
}

function jumpInFile(delta: 1 | -1): void {
  const c = caret();
  if (!c) return;
  const lines = bookmarksIn(c.path)
    .map((b) => b.line)
    .sort((a, b) => a - b);
  if (!lines.length) return;
  const next = delta > 0 ? (lines.find((l) => l > c.line) ?? lines[0]) : ([...lines].reverse().find((l) => l < c.line) ?? lines[lines.length - 1]);
  void openFileAt(c.path, next + 1);
}
action("GotoNextBookmark", () => jumpInFile(1));
action("GotoPreviousBookmark", () => jumpInFile(-1));

if (typeof document !== "undefined") {
  const host = document.createElement("div");
  host.id = "bookmark-mnemonics";
  document.body.appendChild(host);
  render(() => <MnemonicHost />, host);
}
