/**
 * File | File Properties | Override File Type, Revert File Type Override and Associate with
 * File Type… (PHPStorm's file type actions), also in the editor tab and Project view context
 * menus and Find Action.
 *
 * An override is an input of the Rust detection (`forge_filetypes::overrides`): it is stored
 * in the project's `fileTypes.overrides` setting, wins over names, extensions and `#!` lines,
 * and every reader of the file type (highlighting, LSP routing, structure, inspections) sees
 * it. Overrides follow renames and moves and disappear with deleted files.
 */
import { createMemo, createSignal, For, onMount, Show } from "solid-js";
import { render } from "solid-js/web";
import { FileType } from "lucide-solid";
import { filetypesApi, type OverrideChoices, type OverrideEdit } from "@/ipc/filetypes";
import type { SettingObject } from "@/ipc/settings";
import type { FileEntry } from "@/ipc/workspace";
import { fileTypeConfig, fileTypeOf } from "@/editor/fileTypes";
import { registerKeymapActions, showBalloon } from "@/keymap";
import { registerCommand } from "@/state/commands";
import { editor, isCustomTab, onPathMoved } from "@/state/editor";
import { getSetting, saveSetting, settingsState } from "@/state/settings";
import { registerContextMenuProvider, item, type MenuEntry } from "@/features/contextMenu/state";
import { registerMainMenuEntries } from "@/features/mainMenu";
import { action as menuAction } from "@/features/mainMenu/model";
import { createFuzzyFilter } from "@/features/search/fuzzyFilter";
import { Highlight } from "@/features/search/SearchEverywhere";
import { t } from "@/i18n";

const OVERRIDES = "fileTypes.overrides";
const ASSOCIATIONS = "fileTypes.associations";

const GROUP = "Main Menu/File/File Properties";
registerKeymapActions([
  { id: "OverrideFileTypeAction", title: "Override File Type", group: GROUP },
  { id: "RevertFileTypeAction", title: "Revert File Type Override", group: GROUP },
  { id: "AssociateWithFileType", title: "Associate with File Type...", group: GROUP },
]);

const message = (e: unknown) => String(e instanceof Error ? e.message : e);

/** A file of the project or a library, not a tool tab, scratch view or console. */
const isFile = (path: string | null | undefined): path is string => !!path && !isCustomTab(path) && !path.includes("://");

const overrides = () => getSetting<SettingObject[]>(OVERRIDES, []);

export const isOverridden = (path: string): boolean => !!fileTypeOf(path).overridden;

async function editOverrides(edit: OverrideEdit): Promise<void> {
  const next = await filetypesApi.editOverrides(overrides(), edit);
  await saveSetting(OVERRIDES, next as unknown as SettingObject[]);
}

export async function overrideFileType(path: string, fileType: string): Promise<void> {
  try {
    await editOverrides({ op: "set", path, fileType });
  } catch (e) {
    showBalloon(t("editor.fileType.failed"), message(e), 6000);
  }
}

export async function revertFileTypeOverride(path: string): Promise<void> {
  try {
    await editOverrides({ op: "unset", path });
  } catch (e) {
    showBalloon(t("editor.fileType.failed"), message(e), 6000);
  }
}

/** Associate with File Type: add the file's pattern (`*.ext` or its name) to a type. */
async function associate(path: string, fileType: string, pattern: string): Promise<void> {
  const list = getSetting<SettingObject[]>(ASSOCIATIONS, []).filter((a) => !(a.fileType === fileType && a.pattern === pattern));
  try {
    await saveSetting(ASSOCIATIONS, [...list, { fileType, pattern, action: "add" }]);
    showBalloon(t("editor.fileType.associated", { pattern }), path);
  } catch (e) {
    showBalloon(t("editor.fileType.failed"), message(e), 6000);
  }
}

// ---- Popup --------------------------------------------------------------------------------

type Mode = "override" | "associate";
interface PopupState {
  path: string;
  mode: Mode;
  data: OverrideChoices;
}
const [popup, setPopup] = createSignal<PopupState | null>(null);

export { popup as fileTypePopup };

/** Open the searchable list of every file type for `path`. */
export async function openFileTypePopup(path: string, mode: Mode = "override"): Promise<void> {
  try {
    const data = await filetypesApi.overrideChoices({ files: [{ path }], ...fileTypeConfig() });
    // Associate with File Type offers real types only.
    if (mode === "associate") data.choices = data.choices.filter((c) => c.id !== "text");
    setPopup({ path, mode, data });
  } catch (e) {
    showBalloon(t("editor.fileType.failed"), message(e), 6000);
  }
}

const closePopup = () => setPopup(null);

/** Display name of an inactive plugin. */
const pluginName = (id: string) => settingsState.plugins.find((p) => p.id === id)?.name ?? id;

function FileTypePopup(props: { state: PopupState }) {
  let input!: HTMLInputElement;
  let list!: HTMLDivElement;
  const [query, setQuery] = createSignal("");
  const [index, setIndex] = createSignal(0);
  const choices = () => props.state.data.choices;
  const hits = createFuzzyFilter(() => ({ query: query(), candidates: choices().map((c) => [c.name, c.id]) }));
  const shown = createMemo(() =>
    hits()
      .filter((h) => h.index < choices().length)
      .map((h) => ({ c: choices()[h.index], positions: h.text === 0 ? h.positions : [] })),
  );
  const fileName = () => props.state.path.split("/").pop() ?? props.state.path;
  const current = () => props.state.data.current ?? fileTypeOf(props.state.path).id;
  onMount(() => {
    input.focus();
    // Start on the current type.
    const i = shown().findIndex((x) => x.c.id === current());
    if (i >= 0) setIndex(i);
    requestAnimationFrame(() => list.querySelector(`[data-index="${index()}"]`)?.scrollIntoView({ block: "nearest" }));
  });

  function pick(i = index()) {
    const c = shown()[i]?.c;
    if (!c) return;
    const { path, mode, data } = props.state;
    closePopup();
    if (mode === "associate") void associate(path, c.id, data.pattern);
    else void overrideFileType(path, c.id);
  }

  // List navigation inside the popup's own input (not a global shortcut).
  function onKeyDown(e: KeyboardEvent) {
    const n = shown().length;
    if (e.key === "Escape") closePopup();
    else if (e.key === "ArrowDown") setIndex((i) => (n ? (i + 1) % n : 0));
    else if (e.key === "ArrowUp") setIndex((i) => (n ? (i - 1 + n) % n : 0));
    else if (e.key === "Enter") pick();
    else return;
    e.preventDefault();
    e.stopPropagation();
    list.querySelector(`[data-index="${index()}"]`)?.scrollIntoView({ block: "nearest" });
  }

  const title = () =>
    props.state.mode === "associate"
      ? t("editor.fileType.associateTitle", { pattern: props.state.data.pattern })
      : t("editor.fileType.title", { file: fileName() });

  return (
    <div class="popup-backdrop" onMouseDown={(e) => e.target === e.currentTarget && closePopup()}>
      <div class="popup se-popup ft-override-popup" role="dialog" aria-label={title()} data-testid="file-type-override-popup" onKeyDown={onKeyDown}>
        <div class="ft-override-title">{title()}</div>
        <label class="se-search">
          <FileType size={15} class="muted" />
          <input
            ref={input}
            class="se-input"
            value={query()}
            placeholder={t("editor.fileType.search")}
            spellcheck={false}
            onInput={(e) => {
              setQuery(e.currentTarget.value);
              setIndex(0);
            }}
          />
        </label>
        <div class="se-results" ref={list}>
          <For each={shown()} fallback={<div class="se-empty">{t("editor.fileType.noMatch")}</div>}>
            {(x, i) => (
              <div
                class="se-row"
                classList={{ selected: i() === index() }}
                data-index={i()}
                onMouseMove={() => setIndex(i())}
                onClick={() => pick(i())}
              >
                <span class="se-icon">
                  <FileType size={14} class="muted" />
                </span>
                <span class="se-label">
                  <Highlight text={x.c.name} positions={x.positions} />
                </span>
                <Show when={x.c.id === current()}>
                  <span class="se-detail">{t("editor.fileType.current")}</span>
                </Show>
                <Show when={x.c.inactivePlugin}>
                  <span class="se-hint" title={t("editor.fileType.inactiveHint", { plugin: pluginName(x.c.inactivePlugin!) })}>
                    {t("editor.fileType.inactive", { plugin: pluginName(x.c.inactivePlugin!) })}
                  </span>
                </Show>
              </div>
            )}
          </For>
        </div>
        <div class="se-footer">{props.state.mode === "associate" ? t("editor.fileType.associateFooter") : t("editor.fileType.footer")}</div>
      </div>
    </div>
  );
}

if (typeof document !== "undefined") {
  const host = document.createElement("div");
  host.id = "file-type-override-overlay";
  document.body.appendChild(host);
  render(() => <Show when={popup()}>{(state) => <FileTypePopup state={state()} />}</Show>, host);
}

// ---- Actions and menus --------------------------------------------------------------------

const withActive = (fn: (path: string) => void) => () => {
  const path = editor.active;
  if (isFile(path)) fn(path);
  else showBalloon(t("editor.fileType.noFile"), t("editor.fileType.noFileHint"));
};

registerCommand({ id: "OverrideFileTypeAction", title: "Override File Type", category: "File", run: withActive((p) => void openFileTypePopup(p)) });
registerCommand({
  id: "RevertFileTypeAction",
  title: "Revert File Type Override",
  category: "File",
  run: withActive((p) => (isOverridden(p) ? void revertFileTypeOverride(p) : showBalloon(t("editor.fileType.notOverridden"), p))),
});
registerCommand({
  id: "AssociateWithFileType",
  title: "Associate with File Type...",
  category: "File",
  run: withActive((p) => void openFileTypePopup(p, "associate")),
});

registerMainMenuEntries("File/File Properties", [menuAction("RevertFileTypeAction")], {
  after: "OverrideFileTypeAction",
  when: () => isFile(editor.active) && isOverridden(editor.active),
});

/** Entries of the editor tab and Project view menus for one file. */
export function fileTypeMenuEntries(path: string): MenuEntry[] {
  const entries: MenuEntry[] = [item(t("editor.fileType.menu.override"), () => void openFileTypePopup(path), "OverrideFileTypeAction")];
  if (isOverridden(path)) entries.push(item(t("editor.fileType.menu.revert"), () => void revertFileTypeOverride(path), "RevertFileTypeAction"));
  // PHPStorm offers the association for files it does not recognise.
  else if (fileTypeOf(path).id === null) entries.push(item(t("editor.fileType.menu.associate"), () => void openFileTypePopup(path, "associate"), "AssociateWithFileType"));
  return entries;
}

registerContextMenuProvider<string>("editor.tab", (path) => (isFile(path) ? fileTypeMenuEntries(path) : []));
registerContextMenuProvider<FileEntry | null>("project", (entry) =>
  entry && entry.kind === "file" && isFile(entry.path) ? fileTypeMenuEntries(entry.path) : [],
);

// Overrides follow renames and moves and go away with deleted files.
onPathMoved((from, to) => {
  if (!overrides().some((o) => o.path === from || String(o.path).startsWith(`${from}/`))) return;
  void editOverrides({ op: "moved", from, to }).catch((e) => console.warn("file type overrides not updated", e));
});
