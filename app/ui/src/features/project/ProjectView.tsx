import { createResource, createSignal, For, Show } from "solid-js";
import { ChevronDown, ChevronRight } from "lucide-solid";
import { workspaceApi, type FileEntry } from "@/ipc/workspace";
import { workspace } from "@/state/workspace";
import { editor, openFile } from "@/state/editor";
import { statusOf, dirHasChanges } from "@/state/vcs";
import { FileIcon } from "@/shell/FileIcon";

const [expanded, setExpanded] = createSignal<Set<string>>(new Set(["app", "app/Models"]));

const toggle = (path: string) =>
  setExpanded((prev) => {
    const next = new Set(prev);
    next.has(path) ? next.delete(path) : next.add(path);
    return next;
  });

function TreeNode(props: { entry: FileEntry; depth: number }) {
  const isDir = () => props.entry.kind === "dir";
  const isOpen = () => expanded().has(props.entry.path);
  const status = () => (isDir() ? (dirHasChanges(props.entry.path) ? "modified" : undefined) : statusOf(props.entry.path));
  return (
    <>
      <div
        class="tree-row"
        classList={{ selected: editor.active === props.entry.path }}
        style={{ "padding-left": `${8 + props.depth * 16}px` }}
        data-path={props.entry.path}
        onClick={() => (isDir() ? toggle(props.entry.path) : openFile(props.entry.path))}
      >
        <span class="tree-chevron">
          <Show when={isDir()}>{isOpen() ? <ChevronDown size={14} /> : <ChevronRight size={14} />}</Show>
        </span>
        <FileIcon name={props.entry.name} dir={isDir()} open={isOpen()} />
        <span class={`tree-label ${status() ? `vcs-${status()}` : ""}`}>{props.entry.name}</span>
      </div>
      <Show when={isDir() && isOpen()}>
        <DirChildren path={props.entry.path} depth={props.depth + 1} />
      </Show>
    </>
  );
}

function DirChildren(props: { path: string; depth: number }) {
  const [entries] = createResource(() => props.path, (p) => workspaceApi.listDir(p));
  return <For each={entries() ?? []}>{(e) => <TreeNode entry={e} depth={props.depth} />}</For>;
}

export function ProjectView() {
  return (
    <div class="project-tree">
      <Show when={workspace()}>
        {(ws) => (
          <>
            <div class="tree-row tree-root" style={{ "padding-left": "8px" }}>
              <span class="tree-chevron">
                <ChevronDown size={14} />
              </span>
              <FileIcon name={ws().name} dir open />
              <span class="tree-label strong">{ws().name}</span>
              <span class="tree-hint">{ws().root}</span>
            </div>
            <DirChildren path="" depth={1} />
          </>
        )}
      </Show>
    </div>
  );
}
