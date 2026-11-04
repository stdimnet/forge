/**
 * The Settings dialog (Ctrl+Alt+S): searchable page tree on the left, the selected page on
 * the right, OK / Cancel / Apply at the bottom, like PHPStorm.
 */
import { createEffect, createMemo, createSignal, For, on, onCleanup, onMount, Show } from "solid-js";
import { Dynamic } from "solid-js/web";
import { ChevronDown, ChevronRight, CircleHelp, FolderGit2, FolderPlus, Search, X } from "lucide-solid";
import { settingsPages } from "@/features/settingsPages";
import { settingsState } from "@/state/settings";
import {
  applyChanges,
  applying,
  closeSettings,
  dialogClosed,
  dialogOpened,
  error,
  forNewProjects,
  hasChanges,
  isOpen,
  pageDiffersFromDefaults,
  pageModified,
  query,
  resetPage,
  restorePageDefaults,
  selected,
  setQuery,
  setSelected,
  type SettingsDialogProps,
} from "./dialogState";
import { dialogSearch, Hl, nodeMatches } from "./dialogSearch";
import { nodesById, type TreeNode } from "./search";
import { t } from "@/i18n";
import { navigate } from "@/features/project/treeNavigation";
import { tabbables } from "@/features/accessibility/dialogs";

function TreeRow(props: {
  node: TreeNode;
  depth: number;
  expanded: (id: string) => boolean;
  toggle: (id: string) => void;
}) {
  const hasChildren = () => props.node.children.length > 0;
  const modified = () => props.node.pages.some(pageModified);
  return (
    <>
      <div
        class="set-tree-row"
        classList={{
          selected: selected() === props.node.id,
          modified: modified(),
          match: nodeMatches(props.node.id),
        }}
        style={{ "padding-left": `${8 + props.depth * 16}px` }}
        data-node={props.node.id}
        data-depth={props.depth}
        data-dir={hasChildren() ? "" : undefined}
        data-expanded={hasChildren() && props.expanded(props.node.id) ? "" : undefined}
        role="treeitem"
        aria-level={props.depth + 1}
        aria-selected={selected() === props.node.id}
        aria-expanded={hasChildren() ? props.expanded(props.node.id) : undefined}
        tabIndex={selected() === props.node.id ? 0 : -1}
        onClick={() => {
          setSelected(props.node.id);
          if (hasChildren() && !props.node.page) props.toggle(props.node.id);
        }}
      >
        <span
          class="set-tree-chevron"
          aria-hidden="true"
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

/** Content of the Settings dialog; the window frame (title bar) comes from `WindowFrame`. */
export function SettingsDialog(props: { props: SettingsDialogProps }) {
  dialogOpened(props.props ?? {});
  onCleanup(dialogClosed);
  // Tree, filtering and matches come from Rust (`settings_search`).
  const tree = () => dialogSearch().response.tree;
  const visibleTree = () => dialogSearch().response.visible;
  const nodes = createMemo(() => nodesById(tree()));
  const pageById = (id: string) => settingsPages().find((p) => p.id === id);
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

  const selectedPath = createMemo(() =>
    (dialogSearch().response.breadcrumbs[selected()] ?? []).flatMap((id) => nodes().get(id) ?? []),
  );
  const current = () => selectedPath()[selectedPath().length - 1];
  const currentPageId = () => (current()?.page ? current()!.id : undefined);
  const pageHasProjectSettings = () => settingsState.schema.some((s) => s.group === currentPageId() && s.project);
  const projectScoped = () => pageHasProjectSettings() && settingsState.hasProject && !forNewProjects();
  const newProjectsScoped = () => pageHasProjectSettings() && (forNewProjects() || !settingsState.hasProject);

  // Searching selects the first page that matches if the current one does not.
  createEffect(
    on(dialogSearch, (result) => {
      if (!query().trim() || nodeMatches(selected())) return;
      if (result.response.firstMatch) setSelected(result.response.firstMatch);
    }),
  );
  // Fall back to the first page if the remembered page no longer exists.
  createEffect(() => {
    if (isOpen() && dialogSearch().ready && !current() && tree().length) {
      setSelected(dialogSearch().response.firstMatch ?? tree()[0].id);
    }
  });

  let searchEl!: HTMLInputElement;
  let treeEl!: HTMLDivElement;
  /** Arrow keys, Home/End and Enter in the page tree (PHPStorm's tree keys); Enter on a page moves into it. */
  const onTreeKey = (e: KeyboardEvent) => {
    const rows = [...treeEl.querySelectorAll<HTMLElement>(".set-tree-row[data-node]")];
    const action = navigate(
      rows.map((r) => ({
        path: r.dataset.node!,
        isDir: r.dataset.dir !== undefined,
        expanded: r.dataset.expanded !== undefined,
        depth: Number(r.dataset.depth ?? 0),
      })),
      selected(),
      e.key,
    );
    if (action.type === "none") return;
    e.preventDefault();
    e.stopPropagation();
    if (action.type === "select") {
      setSelected(action.path);
      queueMicrotask(() => treeEl.querySelector<HTMLElement>(`[data-node="${CSS.escape(action.path)}"]`)?.focus());
    } else if (action.type === "expand" || action.type === "collapse") toggle(action.path);
    else tabbables(contentEl)[0]?.focus();
  };
  let contentEl!: HTMLDivElement;
  onMount(() => queueMicrotask(() => searchEl?.focus()));

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
    <div class="set-dialog" onKeyDown={onKeyDown}>
      <div class="set-body">
        <aside class="set-sidebar">
          <div class="set-search">
            <Search size={14} class="muted" />
            <input
              ref={searchEl}
              placeholder={t("settings.search")}
              aria-label={t("settings.search")}
              value={query()}
              spellcheck={false}
              onInput={(e) => setQuery(e.currentTarget.value)}
            />
            <Show when={searching()}>
              <button class="set-search-clear" title={t("common.clear")} onClick={() => setQuery("")}>
                <X size={13} />
              </button>
            </Show>
          </div>
          <div class="set-tree" role="tree" aria-label={t("a11y.settingsTree")} ref={treeEl} onKeyDown={onTreeKey}>
            <For each={visibleTree()} fallback={<div class="set-empty">{t("common.nothingFound")}</div>}>
              {(node) => <TreeRow node={node} depth={0} expanded={expanded} toggle={toggle} />}
            </For>
          </div>
        </aside>
        <div class="set-main" role="region" aria-label={t("a11y.settingsPage")}>
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
                <span class="set-scope" title={t("settings.scope.project.tooltip")}>
                  <FolderGit2 size={13} /> {t("settings.scope.project")}
                </span>
              </Show>
              <Show when={newProjectsScoped()}>
                <span class="set-scope" title={t("settings.scope.newProjects.tooltip")}>
                  <FolderPlus size={13} /> {t("settings.scope.newProjects")}
                </span>
              </Show>
            </div>
            <Show when={currentPageId() && pageModified(currentPageId()!)}>
              <button class="set-reset" onClick={() => resetPage(currentPageId()!)}>
                {t("settings.reset")}
              </button>
            </Show>
            <Show when={currentPageId() && pageDiffersFromDefaults(currentPageId()!)}>
              <button
                class="set-reset"
                data-testid="restore-page-defaults"
                title={t("settings.restoreDefaults.tooltip")}
                onClick={() => restorePageDefaults(currentPageId()!)}
              >
                {t("settings.restoreDefaults")}
              </button>
            </Show>
          </div>
          <div class="set-content" ref={contentEl}>
            <Show when={current()}>
              {(node) => (
                <Show when={node().page && pageById(node().id)} fallback={<GroupPage node={node()} />}>
                  {(page) => <Dynamic component={page().component} />}
                </Show>
              )}
            </Show>
          </div>
        </div>
      </div>
      <div class="set-footer">
        <button class="icon-btn" title={t("common.help")} aria-label={t("common.help")}>
          <CircleHelp size={16} />
        </button>
        <Show when={error()}>
          <span class="set-error">{error()}</span>
        </Show>
        <div class="set-footer-buttons">
          <button class="btn-primary set-btn" disabled={applying()} onClick={ok}>
            {t("common.ok")}
          </button>
          <button class="btn-secondary set-btn" onClick={closeSettings}>
            {t("common.cancel")}
          </button>
          <button
            class="btn-secondary set-btn"
            disabled={!hasChanges() || applying()}
            onClick={() => void applyChanges()}
          >
            {t("common.apply")}
          </button>
        </div>
      </div>
    </div>
  );
}
