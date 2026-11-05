/**
 * LightEdit (PHPStorm's lightweight editor): files opened from outside a project ("Open With
 * → Forge" in the file manager, `forge -e file`, a file outside the open project) open as
 * tabs of one detached editor window, without a project, tool windows or indexing. A file
 * opened again (also while Forge runs) becomes the active tab of the same window.
 *
 * Rust owns the window and its tabs (`commands/light_edit.rs`, `forge_window::light_edit`);
 * this window renders the editor with the basic actions: Open File…, Save All, Open
 * Project… and closing tabs. Files are read and written through the LightEdit commands, in
 * the encoding and with the line separator they had. Browser mode:
 * `?dialog=light-edit&file=/home/me/notes.md&file=/etc/hosts` shows the window as if those
 * files were opened from the file manager.
 */
import { createEffect, createMemo, For, on, onCleanup, onMount, Show } from "solid-js";
import { FilePlus2, FolderOpen, Save } from "lucide-solid";
import { isTauri } from "@/ipc";
import { LIGHT_EDIT_PREFIX, lightEditApi, type LightEditSnapshot } from "@/ipc/lightEdit";
import { activateOpenTab, editor, openFile, registerFileSystem, saveFile, isDirty } from "@/state/editor";
import { openFileAt } from "@/editor/views";
import { showBalloon } from "@/keymap";
import { EditorArea } from "@/shell/EditorArea";
import { closeDialog, onDialogCancel, registerDialog } from "@/shell/window/dialogs";
import { dialogBoot } from "@/shell/window/mode";
import { t } from "@/i18n";
import { closedTabs, isLightEditTab, planSync } from "./sync";
import "./lightEdit.css";

export const LIGHT_EDIT_ID = "light-edit";

registerFileSystem({
  prefix: LIGHT_EDIT_PREFIX,
  read: (path) => lightEditApi.read(path),
  write: (path, content) => lightEditApi.write(path, content),
  // The path as the OS spells it, folder by folder.
  breadcrumbs: (path) => path.slice(LIGHT_EDIT_PREFIX.length).split("/").filter(Boolean),
});

const message = (e: unknown) => String((e as Error)?.message ?? e);

function LightEditWindow() {
  let snapshot: LightEditSnapshot = { files: [], active: null, reveal: null };
  const lightTabs = createMemo(() => editor.tabs.map((tab) => tab.path).filter(isLightEditTab), undefined, {
    equals: (a, b) => a.length === b.length && a.every((id, i) => id === b[i]),
  });

  async function apply(next: LightEditSnapshot): Promise<void> {
    snapshot = next;
    const plan = planSync(next, lightTabs());
    // Each new tab opens next to the active one: activate them in turn to keep their order.
    for (const id of plan.toOpen) {
      await openFile(id).catch((e) => showBalloon(t("tools.lightEdit.cannotOpenTheFile"), message(e)));
    }
    if (plan.show) {
      if (plan.show.line) await openFileAt(plan.show.id, plan.show.line, plan.show.column);
      else activateOpenTab(plan.show.id);
    }
  }

  onMount(() => {
    void lightEditApi.state().then(apply);
    const off = lightEditApi.onChanged((s) => void apply(s));
    onCleanup(() => void off.then((unlisten) => unlisten()));
  });

  // Tabs closed in the editor are closed in Rust too (their files can no longer be written).
  createEffect(
    on(lightTabs, (now, before) => {
      for (const id of closedTabs(before ?? [], now)) void lightEditApi.close(id).catch(() => undefined);
    }),
  );
  createEffect(
    on(
      () => editor.active,
      (active) => {
        if (active && isLightEditTab(active)) void lightEditApi.activate(active).catch(() => undefined);
      },
    ),
  );

  const activePath = () => snapshot.files.find((f) => f.id === editor.active)?.path ?? editor.active?.slice(LIGHT_EDIT_PREFIX.length) ?? "";
  createEffect(() => {
    const name = editor.tabs.find((tab) => tab.path === editor.active)?.name;
    document.title = name ? `${name} – ${t("tools.lightEdit.title")}` : t("tools.lightEdit.title");
  });

  async function openFiles(): Promise<void> {
    try {
      let paths: string[];
      if (isTauri()) {
        const { open } = await import("@tauri-apps/plugin-dialog");
        const picked = await open({ multiple: true, directory: false, title: t("tools.lightEdit.openFile") });
        paths = Array.isArray(picked) ? picked : picked ? [picked] : [];
      } else {
        paths = ["/home/me/notes/todo.md"];
      }
      if (paths.length) await apply(await lightEditApi.open(paths));
    } catch (e) {
      showBalloon(t("tools.lightEdit.cannotOpenTheFile"), message(e));
    }
  }

  async function saveAll(): Promise<void> {
    const dirty = editor.tabs.filter((tab) => isLightEditTab(tab.path) && isDirty(tab));
    await Promise.all(dirty.map((tab) => saveFile(tab.path, "plain")));
  }

  async function openProject(): Promise<void> {
    try {
      let dir: string | null = "~/Projects/laravel-shop";
      if (isTauri()) {
        const { open } = await import("@tauri-apps/plugin-dialog");
        const picked = await open({ directory: true, multiple: false, title: t("tools.lightEdit.openProject") });
        dir = typeof picked === "string" ? picked : null;
      }
      if (dir) await lightEditApi.openProject(dir);
    } catch (e) {
      showBalloon(t("tools.lightEdit.cannotOpenTheProject"), message(e));
    }
  }

  // Closing the window (title bar, Alt+F4) saves the files first, like the IDE window.
  const off = onDialogCancel(LIGHT_EDIT_ID, () => {
    void saveAll()
      .catch((e) => showBalloon(t("tools.lightEdit.cannotSave"), message(e)))
      .finally(() => closeDialog(LIGHT_EDIT_ID, null));
  });
  onCleanup(off);

  const actions = [
    { id: "open", label: () => t("tools.lightEdit.openFile"), icon: FilePlus2, run: openFiles },
    { id: "save", label: () => t("tools.lightEdit.saveAll"), icon: Save, run: saveAll },
    { id: "project", label: () => t("tools.lightEdit.openProject"), icon: FolderOpen, run: openProject },
  ];

  return (
    <div class="light-edit" data-light-edit>
      <div class="light-edit-toolbar" role="toolbar" aria-label={t("tools.lightEdit.title")}>
        <For each={actions}>
          {(a) => (
            <button class="light-edit-action" data-action={a.id} onClick={() => void a.run()}>
              <a.icon size={14} />
              <span>{a.label()}</span>
            </button>
          )}
        </For>
      </div>
      <Show
        when={lightTabs().length > 0}
        fallback={
          <div class="light-edit-empty">
            <div class="light-edit-empty-title">{t("tools.lightEdit.noFilesOpen")}</div>
            <div class="light-edit-empty-hint">{t("tools.lightEdit.noFilesHint")}</div>
          </div>
        }
      >
        <div class="light-edit-editor detached-editor">
          <EditorArea />
        </div>
      </Show>
      <div class="light-edit-status" role="status">
        <span class="light-edit-mode">{t("tools.lightEdit.mode")}</span>
        <span class="light-edit-path" title={activePath()}>
          {activePath()}
        </span>
      </div>
    </div>
  );
}

registerDialog({
  id: LIGHT_EDIT_ID,
  title: () => t("tools.lightEdit.title"),
  width: 1000,
  height: 720,
  minWidth: 480,
  minHeight: 320,
  modal: false,
  standalone: true,
  component: LightEditWindow,
});

/** This window is the LightEdit window. */
export const isLightEditWindow = (): boolean => dialogBoot()?.id === LIGHT_EDIT_ID;
