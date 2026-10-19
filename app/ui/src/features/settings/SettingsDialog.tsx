/**
 * The Settings dialog (Ctrl+Alt+S): searchable page tree on the left, the selected page on
 * the right, OK / Cancel / Apply at the bottom, like PHPStorm.
 */
import { createEffect, createMemo, createSignal, For, on, Show } from "solid-js";
import { Dynamic } from "solid-js/web";
import { ChevronDown, ChevronRight, CircleHelp, FolderGit2, Search, X } from "lucide-solid";
import { settingsPages } from "@/features/settingsPages";
import { settingsState } from "@/state/settings";
import {
  applyChanges,
  applying,
  closeSettings,
  error,
  hasChanges,
  isOpen,
  pageModified,
  query,
  resetPage,
  selected,
  setQuery,
  setSelected,
} from "./dialogState";
import { breadcrumb, buildTree, filterTree, flatten, nodeMatches, type TreeNode } from "./model";
import { Hl, schemaText } from "./SchemaPage";
import { pluginText } from "./PluginsPage";

/** Searchable text of a page beyond its title and keywords. */
const extraText = (pageId: string) =>
  pageId === "plugins" ? settingsState.plugins.map(pluginText).join("\n") : schemaText(pageId);

function TreeRow(props: {
  node: TreeNode;
  depth: number;
  expanded: (id: string) => boolean;
  toggle: (id: string) => void;
}) {
  const hasChildren = () => props.node.children.length > 0;
  const modified = () =>
    flatten([props.node]).some((n) => n.page && pageModified(n.page.id));
  return (
    <>
      <div
        class="set-tree-row"
        classList={{
          selected: selected() === props.node.id,
          modified: modified(),
          match: nodeMatches(props.node, query(), extraText),
        }}
        style={{ "padding-left": `${8 + props.depth * 16}px` }}
        data-node={props.node.id}
        onClick={() => {
          setSelected(props.node.id);
          if (hasChildren() && !props.node.page) props.toggle(props.node.id);
        }}
      >
        <span
          class="set-tree-chevron"
          onClick={(e) => {
            e.stopPropagation();
            props.toggle(props.node.id);
          }}
        >
          <Show when={hasChildren()}>
            {props.expanded(props.node.id) ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
          </Show>
        </span>
        <span class="set-tree-title">
          <Hl text={props.node.title} />
        </span>
      </div>
      <Show when={hasChildren() && props.expanded(props.node.id)}>
        <For each={props.node.children}>
          {(child) => (
            <TreeRow node={child} depth={props.depth + 1} expanded={props.expanded} toggle={props.toggle} />
          )}
        </For>
      </Show>
    </>
  );
}

/** Content for a group node (or a page without component): links to its children. */
function GroupPage(props: { node: TreeNode }) {
  return (
    <div class="set-page set-group-page">
      <For each={props.node.children}>
        {(child) => (
          <button class="set-link" onClick={() => setSelected(child.id)}>
            {child.title}
          </button>
        )}
      </For>
    </div>
  );
}

export function SettingsDialog() {
  const tree = createMemo(() => buildTree(settingsPages()));
  const visibleTree = createMemo(() => filterTree(tree(), query(), extraText));
  const [collapsed, setCollapsed] = createSignal<Set<string>>(new Set(["editor.code-style", "lang.php"]));
  const searching = () => query().trim() !== "";
  // While searching every branch is open so all hits are visible.
  const expanded = (id: string) =>
    searching() || !collapsed().has(id) || selectedPath().some((n) => n.id === id && n.id !== selected());
  const toggle = (id: string) =>
    setCollapsed((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });

  const selectedPath = createMemo(() => breadcrumb(tree(), selected()));
  const current = () => selectedPath()[selectedPath().length - 1];
  const currentPageId = () => current()?.page?.id;
  const projectScoped = () =>
    settingsState.schema.some((s) => s.group === currentPageId() && s.project) && settingsState.hasProject;

  // Searching selects the first page that matches if the current one does not.
  createEffect(
    on(query, (q) => {
      if (!q.trim()) return;
      const nodes = flatten(visibleTree());
      if (nodes.some((n) => n.id === selected() && nodeMatches(n, q, extraText))) return;
      const first = nodes.find((n) => n.page && nodeMatches(n, q, extraText)) ?? nodes.find((n) => n.page);
      if (first) setSelected(first.id);
    }),
  );
  // Fall back to the first page if the remembered page no longer exists.
  createEffect(() => {
    if (isOpen() && !current() && tree().length) {
      setSelected(flatten(tree()).find((n) => n.page)?.id ?? tree()[0].id);
    }
  });

  let searchEl!: HTMLInputElement;
  createEffect(() => {
    if (isOpen()) queueMicrotask(() => searchEl?.focus());
  });

  const ok = async () => {
    if (await applyChanges()) closeSettings();
  };
  const onKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      if (searching()) setQuery("");
      else closeSettings();
    }
  };

  return (
    <Show when={isOpen()}>
      <div class="set-overlay" onMouseDown={(e) => e.target === e.currentTarget && closeSettings()}>
        <div class="set-dialog" role="dialog" aria-label="Settings" onKeyDown={onKeyDown}>
          <header class="set-titlebar">
            <span>Settings</span>
            <button class="icon-btn" title="Close" onClick={closeSettings}>
              <X size={15} />
            </button>
          </header>
          <div class="set-body">
            <aside class="set-sidebar">
              <div class="set-search">
                <Search size={14} class="muted" />
                <input
                  ref={searchEl}
                  placeholder="Search settings"
                  value={query()}
                  spellcheck={false}
                  onInput={(e) => setQuery(e.currentTarget.value)}
                />
                <Show when={searching()}>
                  <button class="set-search-clear" title="Clear" onClick={() => setQuery("")}>
                    <X size={13} />
                  </button>
                </Show>
              </div>
              <div class="set-tree">
                <For each={visibleTree()} fallback={<div class="set-empty">Nothing found</div>}>
                  {(node) => <TreeRow node={node} depth={0} expanded={expanded} toggle={toggle} />}
                </For>
              </div>
            </aside>
            <main class="set-main">
              <div class="set-header">
                <div class="set-breadcrumb">
                  <For each={selectedPath()}>
                    {(n, i) => (
                      <>
                        <Show when={i() > 0}>
                          <ChevronRight size={14} class="set-breadcrumb-sep" />
                        </Show>
                        <span
                          classList={{ last: i() === selectedPath().length - 1 }}
                          onClick={() => setSelected(n.id)}
                        >
                          {n.title}
                        </span>
                      </>
                    )}
                  </For>
                  <Show when={projectScoped()}>
                    <span class="set-scope" title="Stored in .forge/settings.toml of the open project">
                      <FolderGit2 size={13} /> For current project
                    </span>
                  </Show>
                </div>
                <Show when={currentPageId() && pageModified(currentPageId()!)}>
                  <button class="set-reset" onClick={() => resetPage(currentPageId()!)}>
                    Reset
                  </button>
                </Show>
              </div>
              <div class="set-content">
                <Show when={current()}>
                  {(node) => (
                    <Show when={node().page} fallback={<GroupPage node={node()} />}>
                      {(page) => <Dynamic component={page().component} />}
                    </Show>
                  )}
                </Show>
              </div>
            </main>
          </div>
          <footer class="set-footer">
            <button class="icon-btn" title="Help">
              <CircleHelp size={16} />
            </button>
            <Show when={error()}>
              <span class="set-error">{error()}</span>
            </Show>
            <div class="set-footer-buttons">
              <button class="btn-primary set-btn" disabled={applying()} onClick={ok}>
                OK
              </button>
              <button class="btn-secondary set-btn" onClick={closeSettings}>
                Cancel
              </button>
              <button
                class="btn-secondary set-btn"
                disabled={!hasChanges() || applying()}
                onClick={() => void applyChanges()}
              >
                Apply
              </button>
            </div>
          </footer>
        </div>
      </div>
    </Show>
  );
}
