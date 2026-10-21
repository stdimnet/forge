/**
 * TODO tool window: the TODO items of the project or of the current file as a tree (root →
 * file → item) like PHPStorm's, with a Filter menu (by pattern), Expand/Collapse All,
 * grouping by directory and Refresh. Clicking an item opens the file at the item.
 */
import { For, Show, createMemo, createSignal } from "solid-js";
import { ChevronDown, ChevronRight, ChevronsDownUp, ChevronsUpDown, Filter, Folder, FolderTree, ListChecks, RotateCw } from "lucide-solid";
import { FileIcon } from "@/shell/FileIcon";
import { openFileAt } from "@/editor/views";
import { activeTab } from "@/state/editor";
import type { TodoFile, TodoItem } from "@/ipc/workbench";
import { showResolvedMenu, type ResolvedEntry } from "@/features/contextMenu/state";
import { filterTodos, hiddenPatterns, patternLabel, patterns, scanProject, setHiddenPatterns, todoResult, todoScanning } from "./store";

export type TodoScope = "project" | "file";

const [scope, setScope] = createSignal<TodoScope>("project");
const [byDirectory, setByDirectory] = createSignal(false);
const [collapsed, setCollapsed] = createSignal<ReadonlySet<string>>(new Set());
const [selected, setSelected] = createSignal<string | null>(null);
export { scope as todoScope, setScope as setTodoScope };

const nameOf = (p: string) => p.slice(p.lastIndexOf("/") + 1);
const dirOf = (p: string) => (p.includes("/") ? p.slice(0, p.lastIndexOf("/")) : "");
const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? "" : "s"}`;

function toggle(key: string): void {
  setCollapsed((s) => {
    const n = new Set(s);
    if (n.has(key)) n.delete(key);
    else n.add(key);
    return n;
  });
}

const currentPath = () => {
  const t = activeTab();
  return t && !t.view ? t.path : undefined;
};

/** The answer for the selected tab and filter. */
const shown = createMemo(() => {
  const r = todoResult();
  if (!r) return null;
  if (scope() === "file") {
    const path = currentPath();
    return path ? filterTodos(r, hiddenPatterns(), path) : { ...r, files: [], count: 0 };
  }
  return filterTodos(r, hiddenPatterns());
});

/** Files grouped by directory (Group By | Directory). */
const directories = createMemo(() => {
  const groups = new Map<string, TodoFile[]>();
  for (const f of shown()?.files ?? []) {
    const d = dirOf(f.path);
    groups.set(d, [...(groups.get(d) ?? []), f]);
  }
  return [...groups.entries()].map(([dir, files]) => ({ dir, files, count: files.reduce((n, f) => n + f.items.length, 0) }));
});

function expandAll(): void {
  setCollapsed(new Set<string>());
}

function collapseAll(): void {
  const keys = new Set<string>();
  for (const f of shown()?.files ?? []) keys.add(`f:${f.path}`);
  for (const d of directories()) keys.add(`d:${d.dir}`);
  setCollapsed(keys);
}

/** Filter TODO Items: show all patterns or only some (PHPStorm's filters). */
function showFilterMenu(e: MouseEvent): void {
  const list = patterns();
  const hidden = hiddenPatterns();
  const entries: ResolvedEntry[] = [
    { kind: "action", label: "Show All", checkable: true, checked: hidden.size === 0, run: () => setHiddenPatterns(new Set<number>()) },
    "separator",
    ...list.map(
      (p, i): ResolvedEntry => ({
        kind: "action",
        label: patternLabel(p.pattern),
        detail: p.pattern,
        checkable: true,
        checked: !hidden.has(i),
        run: () => {
          const next = new Set(hiddenPatterns());
          if (next.has(i)) next.delete(i);
          else next.add(i);
          // Hiding every pattern would show nothing; keep at least one.
          if (next.size < list.length) setHiddenPatterns(next);
        },
      }),
    ),
  ];
  const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
  showResolvedMenu(entries, r.right + 2, r.top);
}

/** The item text with the matched keyword emphasized. */
function ItemText(props: { item: TodoItem }) {
  const head = () => /^\w+/.exec(props.item.text)?.[0] ?? "";
  return (
    <span class="todo-text">
      <span class="todo-keyword">{head()}</span>
      {props.item.text.slice(head().length)}
    </span>
  );
}

function FileNode(props: { file: TodoFile; nested: boolean }) {
  const key = () => `f:${props.file.path}`;
  const open = () => !collapsed().has(key());
  return (
    <>
      <div class="find-node todo-file" classList={{ nested: props.nested }} role="treeitem" aria-expanded={open()} onClick={() => toggle(key())}>
        <Show when={open()} fallback={<ChevronRight size={13} class="muted" />}>
          <ChevronDown size={13} class="muted" />
        </Show>
        <FileIcon name={nameOf(props.file.path)} size={14} />
        <span class="find-file-name">{nameOf(props.file.path)}</span>
        <Show when={!props.nested}>
          <span class="find-dir">{dirOf(props.file.path)}</span>
        </Show>
        <span class="find-count">{plural(props.file.items.length, "item")}</span>
      </div>
      <Show when={open()}>
        <For each={props.file.items}>
          {(item) => {
            const id = `${props.file.path}:${item.line}:${item.column}`;
            return (
              <div
                class="find-node todo-item"
                classList={{ nested: props.nested, selected: selected() === id }}
                role="treeitem"
                title={`${props.file.path}:${item.line + 1}`}
                onClick={() => {
                  setSelected(id);
                  void openFileAt(props.file.path, item.line + 1, item.column + 1, { focus: false });
                }}
                onDblClick={() => void openFileAt(props.file.path, item.line + 1, item.column + 1)}
              >
                <span class="todo-pos">
                  ({item.line + 1}, {item.column + 1})
                </span>
                <ItemText item={item} />
              </div>
            );
          }}
        </For>
      </Show>
    </>
  );
}

export function TodoTabs() {
  return (
    <div class="run-tabs" role="tablist">
      <For each={[["project", "Project"], ["file", "Current File"]] as const}>
        {([id, label]) => (
          <div class="run-tab" classList={{ active: scope() === id }} role="tab" aria-selected={scope() === id} onClick={() => setScope(id)}>
            <span class="run-tab-title">{label}</span>
          </div>
        )}
      </For>
    </div>
  );
}

export function TodoView() {
  const rootLabel = () => {
    const r = shown();
    if (!r) return "";
    if (scope() === "file" && !currentPath()) return "No file is open in the editor";
    const where = scope() === "file" ? ` in ${nameOf(currentPath() ?? "")}` : "";
    if (!r.count) return `No TODO items found${where}`;
    return `Found ${plural(r.count, "TODO item")}${scope() === "project" ? ` in ${plural(r.files.length, "file")}` : where}`;
  };
  return (
    <div class="find-window todo-window">
      <div class="find-toolbar" role="toolbar">
        <button class="find-tool" title="Refresh" disabled={todoScanning()} onClick={() => void scanProject()}>
          <RotateCw size={15} />
        </button>
        <button class="find-tool" classList={{ on: hiddenPatterns().size > 0 }} title="Filter TODO Items" onClick={showFilterMenu}>
          <Filter size={15} />
        </button>
        <button
          class="find-tool"
          classList={{ on: byDirectory() }}
          title="Group by Directory"
          aria-pressed={byDirectory()}
          onClick={() => setByDirectory(!byDirectory())}
        >
          <FolderTree size={15} />
        </button>
        <button class="find-tool" title="Expand All" onClick={expandAll}>
          <ChevronsUpDown size={15} />
        </button>
        <button class="find-tool" title="Collapse All" onClick={collapseAll}>
          <ChevronsDownUp size={15} />
        </button>
      </div>
      <Show
        when={shown()}
        fallback={
          <div class="find-empty">
            <ListChecks size={28} class="muted" />
            <div>{todoScanning() ? "Searching for TODO items…" : "TODO items are shown here."}</div>
          </div>
        }
      >
        {(r) => (
          <div class="find-tree" role="tree" aria-label="TODO items">
            <Show when={r().error}>
              <div class="find-node todo-error">{r().error}</div>
            </Show>
            <div class="find-node find-root">
              <span>{rootLabel()}</span>
              <Show when={r().truncated}>
                <span class="find-count">(only the first results are shown)</span>
              </Show>
              <Show when={todoScanning()}>
                <span class="find-count">Updating…</span>
              </Show>
            </div>
            <Show when={byDirectory()} fallback={<For each={r().files}>{(f) => <FileNode file={f} nested={false} />}</For>}>
              <For each={directories()}>
                {(d) => {
                  const key = `d:${d.dir}`;
                  return (
                    <>
                      <div class="find-node find-group" role="treeitem" onClick={() => toggle(key)}>
                        <Show when={!collapsed().has(key)} fallback={<ChevronRight size={13} class="muted" />}>
                          <ChevronDown size={13} class="muted" />
                        </Show>
                        <Folder size={14} class="muted" />
                        <span class="find-group-label">{d.dir || "Project root"}</span>
                        <span class="find-count">{plural(d.count, "item")}</span>
                      </div>
                      <Show when={!collapsed().has(key)}>
                        <For each={d.files}>{(f) => <FileNode file={f} nested />}</For>
                      </Show>
                    </>
                  );
                }}
              </For>
            </Show>
          </div>
        )}
      </Show>
    </div>
  );
}
