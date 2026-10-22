/**
 * "External Libraries" in the Project view, like PHPStorm: the folders of Settings | PHP |
 * Include Path, browsable and opened read-only (`library://file/…`). They are listed by the
 * backend where the project lives (`php_libraries`, `php_library_list`), so remote projects
 * show their host's folders. Changing the include paths or the source roots indexes again;
 * after every index the `#[Language]` parameters go to the language injections.
 */
import { createEffect, createResource, createRoot, For, on, Show } from "solid-js";
import { ChevronDown, ChevronRight, Library } from "lucide-solid";
import { isDialogWindow } from "@/shell/window/mode";
import { phpMetaApi, type LibraryNode, type LibraryRoot } from "@/ipc/phpMeta";
import { editor, openFile } from "@/state/editor";
import { getSetting } from "@/state/settings";
import { isPluginActive } from "@/state/project";
import { workspace } from "@/state/workspace";
import { FileIcon } from "@/shell/FileIcon";
import { projectTree, registerProjectViewSection } from "@/features/project/ProjectView";
import { setInjectionPlaces } from "@/features/injections/highlight";
import { openSettings } from "@/features/settings/dialogState";
import { showResolvedMenu } from "@/features/contextMenu/state";
import { indexWorkspace, lastStats } from "./status";

export const LIBRARIES_ID = "external-libraries://";
const INCLUDE_KEY = "php.includePaths";
const ROOTS_KEY = "php.sourceRoots";

const includePaths = () => getSetting<string[]>(INCLUDE_KEY, []) ?? [];

function Row(props: {
  id: string;
  depth: number;
  dir: boolean;
  name: string;
  hint?: string;
  icon?: "library";
  open?: string;
  dimmed?: boolean;
  onContextMenu?: (e: MouseEvent) => void;
}) {
  const { expanded, setOpen, toggle, selected, setSelected, singleClickOpens } = projectTree;
  const isOpen = () => expanded().has(props.id);
  const activate = () => (props.dir ? toggle(props.id) : props.open && void openFile(props.open));
  return (
    <div
      class="tree-row library-row"
      classList={{ selected: selected() === props.id, "active-file": !!props.open && editor.active === props.open, ignored: !!props.dimmed }}
      style={{ "padding-left": `${8 + props.depth * 16}px` }}
      data-path={props.id}
      data-dir={props.dir ? "" : undefined}
      data-expanded={props.dir && isOpen() ? "" : undefined}
      data-depth={props.depth}
      onClick={() => {
        setSelected(props.id);
        if (singleClickOpens()) activate();
      }}
      onDblClick={() => !singleClickOpens() && activate()}
      onContextMenu={(e) => {
        e.preventDefault();
        e.stopPropagation();
        setSelected(props.id);
        props.onContextMenu?.(e);
      }}
    >
      <span
        class="tree-chevron"
        onClick={(e) => {
          if (!props.dir) return;
          e.stopPropagation();
          setSelected(props.id);
          setOpen(props.id, !isOpen());
        }}
      >
        <Show when={props.dir}>{isOpen() ? <ChevronDown size={14} /> : <ChevronRight size={14} />}</Show>
      </span>
      <Show when={props.icon === "library"} fallback={<FileIcon name={props.name} dir={props.dir} open={isOpen()} dimmed={props.dimmed} />}>
        <span class="library-icon">
          <Library size={15} />
        </span>
      </Show>
      <span class="tree-label">{props.name}</span>
      <Show when={props.hint}>
        <span class="tree-hint">{props.hint}</span>
      </Show>
    </div>
  );
}

/** The entries of a library folder, loaded when it is expanded. */
function Folder(props: { path: string; depth: number }) {
  const [entries] = createResource(() => props.path, (p) => phpMetaApi.libraryList(p).catch((): LibraryNode[] => []));
  return (
    <For each={entries() ?? []}>
      {(e) => {
        const id = `${LIBRARIES_ID}${e.path}`;
        return (
          <>
            <Row id={id} depth={props.depth} dir={e.dir} name={e.name} open={e.open} />
            <Show when={e.dir && projectTree.expanded().has(id)}>
              <Folder path={e.path} depth={props.depth + 1} />
            </Show>
          </>
        );
      }}
    </For>
  );
}

const includeMenu = (e: MouseEvent) =>
  showResolvedMenu([{ kind: "action", label: "Configure PHP Include Paths…", run: () => openSettings("lang.php") }], e.clientX, e.clientY);

/** The "External Libraries" node below the project. */
export function ExternalLibrariesNode() {
  const [roots] = createResource(
    () => (isPluginActive("forge.php") && workspace() ? JSON.stringify(includePaths()) : null),
    () => phpMetaApi.libraries().catch((): LibraryRoot[] => []),
  );
  const open = () => projectTree.expanded().has(LIBRARIES_ID);
  return (
    <Show when={(roots() ?? []).length > 0}>
      <Row id={LIBRARIES_ID} depth={0} dir name="External Libraries" icon="library" onContextMenu={includeMenu} />
      <Show when={open()}>
        <For each={roots() ?? []}>
          {(r) => {
            const id = `${LIBRARIES_ID}${r.path}`;
            return (
              <>
                <Row
                  id={id}
                  depth={1}
                  dir={r.exists}
                  name={r.name}
                  hint={r.exists ? r.path : `${r.path} (not found)`}
                  dimmed={!r.exists}
                  onContextMenu={includeMenu}
                />
                <Show when={r.exists && projectTree.expanded().has(id)}>
                  <Folder path={r.path} depth={2} />
                </Show>
              </>
            );
          }}
        </For>
      </Show>
    </Show>
  );
}

registerProjectViewSection({ id: "external-libraries", order: 90, component: ExternalLibrariesNode });

if (typeof document !== "undefined" && !isDialogWindow()) {
  createRoot(() => {
    // Include paths and source roots changed: index again (the backend gets them first).
    createEffect(
      on(
        () => [JSON.stringify(includePaths()), JSON.stringify(getSetting(ROOTS_KEY, []))],
        () => {
          if (workspace() && isPluginActive("forge.php")) void indexWorkspace();
        },
        { defer: true },
      ),
    );
    // `#[Language]` parameters for the language injections, after every index.
    createEffect(
      on(lastStats, (stats) => {
        if (!stats || !isPluginActive("forge.php")) return setInjectionPlaces([]);
        void phpMetaApi
          .languagePlaces()
          .then(setInjectionPlaces)
          .catch(() => setInjectionPlaces([]));
      }),
    );
  });
}
