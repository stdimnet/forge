/**
 * "Scratches and Consoles" at the bottom of the Project tool window, like PHPStorm: the
 * scratch files (global, from the user config dir) and the open database query consoles.
 */
import { createMemo, createSignal, For, onMount, Show, type JSX } from "solid-js";
import { ChevronDown, ChevronRight, FilePlus2, NotebookPen, Pencil, Play, SquareTerminal, Trash2, Database } from "lucide-solid";
import { editor, openFile } from "@/state/editor";
import { shortcutLabel } from "@/keymap";
import { projectTree } from "@/features/project/ProjectView";
import { openMenu, type MenuItem } from "@/features/run/PopupMenu";
import { CONSOLE_VIEW, consoleSourceId, parseConsolePath, sourceById } from "@/features/database/store";
import type { ScratchFile } from "@/ipc/scratches";
import { FileIcon } from "@/shell/FileIcon";
import { openNewScratchPopup } from "./NewScratchPopup";
import { createBuffer, deleteScratch, openScratch, renameScratch, runScratch, scratchPath, scratches } from "./store";
import { t as tr } from "@/i18n";

export const ROOT_ID = "scratches-and-consoles://";
const SCRATCHES_ID = `${ROOT_ID}scratches`;
const CONSOLES_ID = `${ROOT_ID}consoles`;

/** Scratch file icon: the language's icon with a pencil badge. */
export function ScratchIcon(props: { name: string; size?: number }) {
  return (
    <span class="scratch-icon" style={{ width: `${props.size ?? 15}px`, height: `${props.size ?? 15}px` }}>
      <FileIcon name={props.name} size={props.size ?? 15} />
      <span class="scratch-icon-badge">
        <Pencil size={7} stroke-width={3} />
      </span>
    </span>
  );
}

const [renaming, setRenaming] = createSignal<string | null>(null);

/** Start renaming a scratch in place (context menu "Rename…"). */
export const startRename = (name: string) => setRenaming(name);

interface RowProps {
  id: string;
  depth: number;
  dir?: boolean;
  label: JSX.Element;
  icon: JSX.Element;
  hint?: string;
  activeFile?: boolean;
  onOpen?: () => void;
  onContextMenu?: (e: MouseEvent) => void;
}

function Row(props: RowProps) {
  const { expanded, setOpen, toggle, selected, setSelected, singleClickOpens } = projectTree;
  const isOpen = () => expanded().has(props.id);
  const activate = () => (props.dir ? toggle(props.id) : props.onOpen?.());
  return (
    <div
      class="tree-row scratch-row"
      classList={{ selected: selected() === props.id, "active-file": !!props.activeFile }}
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
      {props.icon}
      {props.label}
      <Show when={props.hint}>
        <span class="tree-hint">{props.hint}</span>
      </Show>
    </div>
  );
}

function RenameInput(props: { name: string }) {
  let input!: HTMLInputElement;
  onMount(() => {
    input.focus();
    // Select the name without its extension, as PHPStorm does.
    const dot = props.name.lastIndexOf(".");
    input.setSelectionRange(0, dot > 0 ? dot : props.name.length);
  });
  let done = false;
  const commit = async (value: string | null) => {
    if (done) return;
    done = true;
    setRenaming(null);
    if (value !== null) await renameScratch(props.name, value);
  };
  return (
    <input
      ref={input}
      class="scratch-rename"
      value={props.name}
      spellcheck={false}
      onClick={(e) => e.stopPropagation()}
      onKeyDown={(e) => {
        e.stopPropagation();
        if (e.key === "Enter") void commit(e.currentTarget.value);
        else if (e.key === "Escape") void commit(null);
      }}
      onBlur={(e) => void commit(e.currentTarget.value)}
    />
  );
}

function scratchMenu(file: ScratchFile, e: MouseEvent) {
  const items: MenuItem[] = [];
  if (file.runner && file.runner !== "sql")
    items.push(
      { label: tr("common.run", { name: file.name }), icon: Play, iconColor: "var(--success)", shortcut: shortcutLabel("RunClass"), run: () => void runScratch(file.name) },
      { separator: true },
    );
  if (file.runner === "sql")
    items.push({ label: file.dataSource ? tr("tools.scratches.openConsole") : tr("tools.scratches.attachDataSourceEllipsis"), icon: Database, run: () => void runScratch(file.name) }, { separator: true });
  items.push(
    { label: tr("tools.scratches.renameEllipsis"), shortcut: shortcutLabel("RenameElement"), run: () => startRename(file.name) },
    { label: tr("common.delete"), icon: Trash2, shortcut: "Delete", run: () => void deleteScratch(file.name) },
  );
  openMenu({ x: e.clientX, y: e.clientY, items });
}

function folderMenu(e: MouseEvent) {
  openMenu({
    x: e.clientX,
    y: e.clientY,
    items: [
      { label: tr("tools.scratches.newScratchFileEllipsis"), icon: FilePlus2, shortcut: shortcutLabel("NewScratchFile"), run: () => void openNewScratchPopup() },
      { label: tr("tools.scratches.newScratchBuffer"), run: () => void createBuffer() },
    ],
  });
}

function ScratchRow(props: { file: ScratchFile; depth: number }) {
  const path = () => scratchPath(props.file.name);
  return (
    <Row
      id={path()}
      depth={props.depth}
      icon={<ScratchIcon name={props.file.name} />}
      label={
        <Show when={renaming() === props.file.name} fallback={<span class="tree-label">{props.file.name}</span>}>
          <RenameInput name={props.file.name} />
        </Show>
      }
      hint={props.file.dataSource ? sourceById(props.file.dataSource)?.name ?? props.file.dataSource : undefined}
      activeFile={editor.active === path()}
      onOpen={() => void openScratch(props.file)}
      onContextMenu={(e) => scratchMenu(props.file, e)}
    />
  );
}

/** Open query consoles (not file backed), grouped by data source. */
function consoleGroups() {
  const groups = new Map<string, { path: string; name: string }[]>();
  for (const t of editor.tabs) {
    if (t.view !== CONSOLE_VIEW || !parseConsolePath(t.path)) continue;
    const id = consoleSourceId(t.path)!;
    groups.set(id, [...(groups.get(id) ?? []), { path: t.path, name: t.name }]);
  }
  return [...groups.entries()].map(([id, consoles]) => ({ id, consoles }));
}

export function ScratchesNode() {
  const { expanded } = projectTree;
  const groups = createMemo(consoleGroups);
  return (
    <div class="scratches-node" data-testid="scratches-node">
      <Row
        id={ROOT_ID}
        depth={0}
        dir
        icon={<NotebookPen size={15} stroke-width={1.75} class="file-icon scratches-root-icon" />}
        label={<span class="tree-label">{tr("tools.scratches.scratchesAndConsoles")}</span>}
      />
      <Show when={expanded().has(ROOT_ID)}>
        <Show when={groups().length}>
          <Row
            id={CONSOLES_ID}
            depth={1}
            dir
            icon={<FileIcon name="Database Consoles" dir open={expanded().has(CONSOLES_ID)} />}
            label={<span class="tree-label">{tr("tools.scratches.databaseConsoles")}</span>}
          />
          <Show when={expanded().has(CONSOLES_ID)}>
            <For each={groups()}>
              {(g) => {
                const id = `${CONSOLES_ID}/${g.id}`;
                return (
                  <>
                    <Row
                      id={id}
                      depth={2}
                      dir
                      icon={<Database size={15} stroke-width={1.75} class="file-icon scratch-ds-icon" />}
                      label={<span class="tree-label">{sourceById(g.id)?.name ?? g.id}</span>}
                    />
                    <Show when={expanded().has(id)}>
                      <For each={g.consoles}>
                        {(c) => (
                          <Row
                            id={c.path}
                            depth={3}
                            icon={<SquareTerminal size={15} stroke-width={1.75} class="file-icon db-tab-icon console" />}
                            label={<span class="tree-label">{c.name}</span>}
                            activeFile={editor.active === c.path}
                            onOpen={() => void openFile(c.path)}
                          />
                        )}
                      </For>
                    </Show>
                  </>
                );
              }}
            </For>
          </Show>
        </Show>
        <Row
          id={SCRATCHES_ID}
          depth={1}
          dir
          icon={<FileIcon name="Scratches" dir open={expanded().has(SCRATCHES_ID)} />}
          label={<span class="tree-label">{tr("tools.scratches.scratches")}</span>}
          onContextMenu={folderMenu}
        />
        <Show when={expanded().has(SCRATCHES_ID)}>
          <For each={scratches()} fallback={<div class="tree-row muted scratch-empty" style={{ "padding-left": `${8 + 2 * 16 + 19}px` }}>{tr("tools.scratches.noScratchesYet")}</div>}>
            {(file) => <ScratchRow file={file} depth={2} />}
          </For>
        </Show>
      </Show>
    </div>
  );
}
