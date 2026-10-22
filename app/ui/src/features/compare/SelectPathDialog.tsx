/**
 * Select Path: the project tree in a dialog, to pick the second file or folder of Compare
 * With… (Ctrl+D with one entry selected).
 */
import { createResource, createSignal, For, onCleanup, onMount, Show } from "solid-js";
import { ChevronDown, ChevronRight } from "lucide-solid";
import { suspendKeymap } from "@/keymap";
import { workspaceApi, type FileEntry } from "@/ipc/workspace";
import { workspace } from "@/state/workspace";
import { FileIcon } from "@/shell/FileIcon";
import { setSelectPathRequest, type SelectPathRequest } from "./state";

const parentsOf = (path: string) => {
  const parts = path.split("/");
  return parts.slice(0, -1).map((_, i) => parts.slice(0, i + 1).join("/"));
};

export function SelectPathDialog(props: { req: SelectPathRequest }) {
  const [selected, setSelected] = createSignal<{ path: string; dir: boolean } | null>(null);
  const [expanded, setExpanded] = createSignal(new Set(props.req.initial ? parentsOf(props.req.initial) : []));
  let dialog!: HTMLDivElement;

  const toggle = (path: string) =>
    setExpanded((prev) => {
      const next = new Set(prev);
      next.has(path) ? next.delete(path) : next.add(path);
      return next;
    });
  const acceptable = () => {
    const s = selected();
    return !!s && s.dir === props.req.dirs;
  };
  const close = () => setSelectPathRequest(null);
  const ok = async () => {
    const s = selected();
    if (!s || !acceptable()) return;
    close();
    await props.req.onOk(s.path);
  };
  const onKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Escape") close();
    else if (e.key === "Enter") void ok();
    else return;
    e.preventDefault();
    e.stopPropagation();
  };

  const release = suspendKeymap();
  onCleanup(release);
  onMount(() => dialog.focus());

  function Children(p: { dir: string; depth: number }) {
    const [entries] = createResource(
      () => p.dir,
      (dir) => workspaceApi.listDir(dir).catch(() => [] as FileEntry[]),
    );
    return (
      <For each={entries() ?? []}>
        {(e) => {
          const isDir = e.kind === "dir";
          const open = () => expanded().has(e.path);
          return (
            <>
              <div
                class="tree-row"
                classList={{ selected: selected()?.path === e.path, ignored: !!e.ignored }}
                style={{ "padding-left": `${8 + p.depth * 16}px` }}
                data-path={e.path}
                onClick={() => setSelected({ path: e.path, dir: isDir })}
                onDblClick={() => (isDir && !props.req.dirs ? toggle(e.path) : (setSelected({ path: e.path, dir: isDir }), void ok()))}
              >
                <span
                  class="tree-chevron"
                  onClick={(ev) => {
                    if (!isDir) return;
                    ev.stopPropagation();
                    toggle(e.path);
                  }}
                >
                  <Show when={isDir}>{open() ? <ChevronDown size={14} /> : <ChevronRight size={14} />}</Show>
                </span>
                <FileIcon name={e.name} dir={isDir} open={open()} dimmed={e.ignored} />
                <span class="tree-label">{e.name}</span>
              </div>
              <Show when={isDir && open()}>
                <Children dir={e.path} depth={p.depth + 1} />
              </Show>
            </>
          );
        }}
      </For>
    );
  }

  return (
    <div class="modal-backdrop" onMouseDown={(e) => e.target === e.currentTarget && close()}>
      <div class="git-dialog select-path-dialog" role="dialog" aria-label={props.req.title} tabIndex={-1} ref={dialog} onKeyDown={onKeyDown}>
        <header class="dialog-title">
          <span>{props.req.title}</span>
        </header>
        <div class="git-dialog-body">
          <div class="select-path-description">{props.req.description}</div>
          <div class="select-path-field">{selected()?.path ?? workspace()?.name ?? ""}</div>
          <div class="select-path-tree">
            <Children dir="" depth={0} />
          </div>
        </div>
        <footer class="git-dialog-actions">
          <button class="btn-secondary" onClick={close}>
            Cancel
          </button>
          <button class="btn-primary" disabled={!acceptable()} onClick={() => void ok()}>
            OK
          </button>
        </footer>
      </div>
    </div>
  );
}
