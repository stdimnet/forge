/**
 * Remote Host tool window (PHPStorm's Tools | Deployment | Browse Remote Host): a server
 * selector over the deployment servers and the lazy file tree of the chosen server. Mapped
 * folders are highlighted, excluded ones dimmed; the context menu edits, transfers,
 * compares, creates, renames and deletes on the server.
 */
import { createEffect, createMemo, For, on, Show } from "solid-js";
import { ChevronDown, ChevronRight, ChevronsDownUp, LoaderCircle, RefreshCw, Server, Settings } from "lucide-solid";
import type { RemoteNode } from "@/ipc/remoteHost";
import { FileIcon } from "@/shell/FileIcon";
import { item, separator, showContextMenu, submenu, type MenuEntry } from "@/features/contextMenu/state";
import { config, openDeploymentSettings, refreshConfig } from "@/features/deployment/store";
import { KindIcon, KIND_LABELS } from "@/features/deployment/DeploymentPage";
import {
  collapseAll,
  compareWithLocal,
  copyPath,
  createEntry,
  currentServer,
  deleteEntry,
  downloadFromHere,
  editRemoteFile,
  refresh,
  renameEntry,
  ROOT,
  select,
  selectServer,
  toggle,
  tree,
  uploadToHere,
} from "./store";

interface Row {
  node: RemoteNode;
  depth: number;
}

function menuFor(node: RemoteNode): MenuEntry[] {
  const mapped = node.local !== null;
  const hint = mapped ? undefined : "Not mapped to a project path (Mappings tab of the server)";
  return [
    ...(node.dir ? [] : [item("Edit Remote File", () => void editRemoteFile(node)), separator]),
    item("Download from Here", () => void downloadFromHere(node), undefined, { disabled: !mapped, hint }),
    item("Upload to Here", () => void uploadToHere(node), undefined, { disabled: !mapped, hint }),
    item("Compare with Local Version", () => void compareWithLocal(node), undefined, { disabled: !mapped || node.dir, hint }),
    separator,
    submenu("New", [item("File", () => void createEntry(false, node.path)), item("Directory", () => void createEntry(true, node.path))]),
    item("Rename…", () => void renameEntry(node), undefined, { disabled: node.path === ROOT }),
    item("Delete…", () => void deleteEntry(node), undefined, { disabled: node.path === ROOT }),
    separator,
    item("Copy Path", () => void copyPath(node)),
    item("Refresh", () => void refresh(node.dir ? node.path : undefined)),
  ];
}

function Toolbar() {
  return (
    <div class="rh-toolbar">
      <Show when={currentServer()} fallback={<span class="muted rh-small">No servers</span>}>
        {(s) => (
          <label class="rh-server" title={`${KIND_LABELS[s().kind]}: ${s().label}`}>
            <KindIcon kind={s().kind} size={14} />
            <select class="rh-select" value={s().id} onChange={(e) => selectServer(e.currentTarget.value)} data-testid="remote-host-server">
              <For each={config()?.servers ?? []}>{(x) => <option value={x.id}>{x.name}</option>}</For>
            </select>
          </label>
        )}
      </Show>
      <span class="rh-spacer" />
      <button class="icon-btn" title="Refresh" disabled={!currentServer()} onClick={() => void refresh()}>
        <RefreshCw size={14} />
      </button>
      <button class="icon-btn" title="Collapse All" disabled={!currentServer()} onClick={collapseAll}>
        <ChevronsDownUp size={15} />
      </button>
      <button class="icon-btn" title="Configure Servers…" onClick={() => openDeploymentSettings()}>
        <Settings size={14} />
      </button>
    </div>
  );
}

export function RemoteHostView() {
  // Show the default server once the configuration is known.
  createEffect(
    on(
      () => currentServer()?.id,
      (id) => {
        if (id && !tree.children[ROOT] && !tree.loading[ROOT]) selectServer(id);
      },
    ),
  );
  createEffect(() => {
    if (!config()) void refreshConfig();
  });

  const rows = createMemo<Row[]>(() => {
    const out: Row[] = [];
    const walk = (path: string, depth: number) => {
      for (const node of tree.children[path] ?? []) {
        out.push({ node, depth });
        if (node.dir && tree.expanded[node.path]) walk(node.path, depth + 1);
      }
    };
    const root = tree.rootNode;
    if (currentServer() && root) {
      out.push({ node: root, depth: 0 });
      if (tree.expanded[ROOT]) walk(ROOT, 1);
    }
    return out;
  });

  const errorRow = (path: string, depth: number) => (
    <Show when={tree.errors[path]}>
      <div class="rh-error" style={{ "padding-left": `${22 + depth * 16}px` }} data-testid="remote-host-error">
        {tree.errors[path]}
      </div>
    </Show>
  );

  return (
    <div class="rh-view">
      <Toolbar />
      <Show
        when={config()?.servers.length}
        fallback={
          <div class="placeholder-panel">
            No deployment servers.{" "}
            <button class="link-btn" onClick={() => openDeploymentSettings()}>
              Configure servers
            </button>
          </div>
        }
      >
        <div class="rh-tree project-tree" tabIndex={0} role="tree" aria-label="Remote Host" data-testid="remote-host-tree">
          <For each={rows()}>
            {(row) => (
              <>
                <div
                  class="tree-row rh-row"
                  classList={{
                    selected: tree.selected === row.node.path,
                    "rh-mapped": row.node.mappingRoot,
                    "rh-excluded": row.node.excluded,
                  }}
                  style={{ "padding-left": `${6 + row.depth * 16}px` }}
                  role="treeitem"
                  aria-expanded={row.node.dir ? !!tree.expanded[row.node.path] : undefined}
                  data-remote-path={row.node.path}
                  data-dir={row.node.dir ? "" : undefined}
                  title={row.node.local !== null ? `Mapped to ${row.node.local || "the project root"}` : row.node.excluded ? "Excluded path" : undefined}
                  onClick={() => select(row.node.path)}
                  onDblClick={() => (row.node.dir ? void toggle(row.node) : void editRemoteFile(row.node))}
                  onContextMenu={(e) => {
                    e.preventDefault();
                    select(row.node.path);
                    showContextMenu(menuFor(row.node), e.clientX, e.clientY);
                  }}
                >
                  <span
                    class="tree-chevron"
                    onClick={(e) => {
                      e.stopPropagation();
                      void toggle(row.node);
                    }}
                  >
                    <Show when={row.node.dir}>
                      <Show when={!tree.loading[row.node.path]} fallback={<LoaderCircle size={12} class="rh-spin" />}>
                        {tree.expanded[row.node.path] ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
                      </Show>
                    </Show>
                  </span>
                  <Show when={row.node.path !== ROOT} fallback={<Server size={15} class="rh-root-icon" />}>
                    <FileIcon name={row.node.name} dir={row.node.dir} open={!!tree.expanded[row.node.path]} size={15} dimmed={row.node.excluded} />
                  </Show>
                  <span class="tree-label" classList={{ strong: row.node.path === ROOT }}>
                    {row.node.name}
                  </span>
                  <Show when={row.node.mappingRoot && row.node.local !== null}>
                    <span class="tree-hint">{row.node.local || "project root"}</span>
                  </Show>
                </div>
                <Show when={row.node.dir && tree.expanded[row.node.path]}>{errorRow(row.node.path, row.depth + 1)}</Show>
              </>
            )}
          </For>
        </div>
      </Show>
    </div>
  );
}
