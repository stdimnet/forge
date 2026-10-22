/**
 * File Structure popup (Navigate | File Structure, Ctrl+F12) like PHPStorm: the current
 * file's structure as a tree with speed search, the node at the caret selected, and the
 * toolbar toggles "Show Inherited" (Ctrl+F12 again), "Show Fields", "Show Lambdas" and
 * "Sort Alphabetically". The rows (toggles, fuzzy filter keeping parents, caret node) come
 * from Rust (`forge_codeintel::file_structure` through WebAssembly); the structure itself
 * from the Structure view's providers (PHP index, LuaForge, web files).
 */
import { createEffect, createMemo, createResource, createSignal, For, on, onMount, Show } from "solid-js";
import { Search } from "lucide-solid";
import { shortcutLabel } from "@/keymap";
import { Highlight } from "@/features/search/SearchEverywhere";
import { SymbolIcon } from "@/features/php/SymbolIcon";
import { loadFileStructure, revealStructureNode, supportsInherited } from "@/features/php/StructureView";
import { fileStructureView, type FileStructureRow } from "@/features/codeStructure/model";

/** The file the popup was opened for. */
export interface FileStructureSession {
  path: string;
  content: string;
  caretLine?: number;
  enclosing: { line: number; openLine?: number }[];
}

const [session, setSession] = createSignal<FileStructureSession | null>(null);
const [showInherited, setShowInherited] = createSignal(false);
const [showFields, setShowFields] = createSignal(true);
const [showLambdas, setShowLambdas] = createSignal(true);
const [sortAlpha, setSortAlpha] = createSignal(false);

export { session as fileStructureSession };

/** Open the popup, or toggle "Show Inherited" when it is open (Ctrl+F12 again). */
export function showFileStructure(s: FileStructureSession): void {
  if (session()) {
    if (supportsInherited(session()!.path)) setShowInherited((v) => !v);
    return;
  }
  setShowInherited(false);
  setSession(s);
}

export const closeFileStructure = () => setSession(null);

const fileName = (path: string) => path.split("/").pop() ?? path;

function Toggle(props: { label: string; hint?: string; checked: boolean; onChange: (v: boolean) => void }) {
  return (
    <label class="se-check fs-toggle" onMouseDown={(e) => e.preventDefault()}>
      <input type="checkbox" checked={props.checked} onChange={(e) => props.onChange(e.currentTarget.checked)} />
      <span>{props.label}</span>
      <Show when={props.hint}>
        <span class="fs-toggle-hint">{props.hint}</span>
      </Show>
    </label>
  );
}

export function FileStructurePopup(props: { session: FileStructureSession }) {
  let input!: HTMLInputElement;
  let list!: HTMLDivElement;
  const [query, setQuery] = createSignal("");
  const [index, setIndex] = createSignal(0);
  const inheritedApplies = () => supportsInherited(props.session.path);

  const [structure] = createResource(
    () => ({ path: props.session.path, content: props.session.content, inherited: inheritedApplies() && showInherited() }),
    async (k) => (await loadFileStructure(k.path, k.content, k.inherited).catch(() => null))?.nodes ?? [],
  );

  const view = createMemo(() =>
    fileStructureView({
      nodes: structure() ?? [],
      query: query(),
      sortAlpha: sortAlpha(),
      showFields: showFields(),
      showInherited: inheritedApplies() && showInherited(),
      showLambdas: showLambdas(),
      caretLine: props.session.caretLine,
      enclosing: props.session.enclosing,
    }),
  );
  const rows = () => view().rows;

  // The view's choice (caret node, or the best match) whenever the rows change.
  createEffect(on(view, (v) => setIndex(v.selected ?? 0)));
  createEffect(() => {
    list?.querySelector(`[data-row="${index()}"]`)?.scrollIntoView({ block: "nearest" });
  });

  onMount(() => input.focus());

  const open = (row: FileStructureRow | undefined) => {
    if (!row) return;
    const file = props.session.path;
    closeFileStructure();
    revealStructureNode(file, row.node);
  };

  // List navigation while the popup has focus (the popup owns these keys, not the keymap).
  function onKeyDown(e: KeyboardEvent) {
    const n = rows().length;
    const page = 10;
    switch (e.key) {
      case "ArrowDown":
        setIndex((i) => (n ? (i + 1) % n : 0));
        break;
      case "ArrowUp":
        setIndex((i) => (n ? (i + n - 1) % n : 0));
        break;
      case "PageDown":
        setIndex((i) => Math.min(n - 1, i + page));
        break;
      case "PageUp":
        setIndex((i) => Math.max(0, i - page));
        break;
      case "Enter":
        open(rows()[index()]);
        break;
      case "Escape":
        closeFileStructure();
        break;
      default:
        return;
    }
    e.preventDefault();
    e.stopPropagation();
  }

  return (
    <div class="popup-backdrop" onMouseDown={(e) => e.target === e.currentTarget && closeFileStructure()}>
      <div class="popup fs-popup" role="dialog" aria-label="File Structure" onKeyDown={onKeyDown}>
        <div class="fs-header">
          <span class="fs-title">{fileName(props.session.path)}</span>
          <div class="fs-toggles">
            <Show when={inheritedApplies()}>
              <Toggle label="Show Inherited" hint={shortcutLabel("FileStructurePopup")} checked={showInherited()} onChange={setShowInherited} />
            </Show>
            <Show when={view().hasFields || !showFields()}>
              <Toggle label="Show Fields" checked={showFields()} onChange={setShowFields} />
            </Show>
            <Show when={view().hasLambdas}>
              <Toggle label="Show Lambdas" checked={showLambdas()} onChange={setShowLambdas} />
            </Show>
            <Toggle label="Sort Alphabetically" checked={sortAlpha()} onChange={setSortAlpha} />
          </div>
        </div>
        <div class="se-search fs-search">
          <Search size={15} class="muted" />
          <input
            ref={input}
            class="se-input"
            placeholder="Type to search"
            spellcheck={false}
            aria-label="Search the file structure"
            value={query()}
            onInput={(e) => setQuery(e.currentTarget.value)}
          />
        </div>
        <div class="fs-tree" ref={list} role="tree">
          <Show
            when={rows().length}
            fallback={<div class="se-empty">{structure.loading ? "Loading…" : query() ? "Nothing found" : "No structure for this file"}</div>}
          >
            <For each={rows()}>
              {(row, i) => (
                <div
                  class="fs-row"
                  classList={{ selected: index() === i(), dimmed: !!query().trim() && !row.matched, inherited: !!row.node.inheritedFrom }}
                  style={{ "padding-left": `${8 + row.depth * 18}px` }}
                  role="treeitem"
                  aria-selected={index() === i()}
                  data-row={i()}
                  title={row.node.inheritedFrom ? `Inherited from ${row.node.inheritedFrom}` : undefined}
                  onMouseMove={() => setIndex(i())}
                  onClick={() => open(row)}
                >
                  <SymbolIcon kind={row.node.kind} visibility={row.node.visibility} modifiers={row.node.modifiers} />
                  <span class="fs-name" classList={{ deprecated: !!row.node.deprecated }}>
                    <Highlight text={row.node.name} positions={row.positions} />
                  </span>
                  <Show when={row.node.detail}>
                    <span class="fs-detail">{row.node.detail}</span>
                  </Show>
                  <Show when={row.node.inheritedFrom}>
                    <span class="fs-owner">↑{row.node.inheritedFrom}</span>
                  </Show>
                </div>
              )}
            </For>
          </Show>
        </div>
      </div>
    </div>
  );
}
