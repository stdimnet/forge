/**
 * "Use path mappings" of Settings | PHP | Servers: the project's folders as a tree with
 * an "Absolute path on the server" cell per row, like PHPStorm. A non-empty cell is a
 * mapping of that folder (the project root included).
 */
import { createResource, createSignal, For, Show } from "solid-js";
import { ChevronDown, ChevronRight, Folder, FolderRoot } from "lucide-solid";
import { workspaceApi } from "@/ipc/workspace";
import { workspace } from "@/state/workspace";
import type { PathMapping } from "./api";

export interface MappingsTreeProps {
  mappings: PathMapping[];
  disabled?: boolean;
  /** Set (non-empty `remote`) or remove (empty) the mapping of project folder `local`. */
  onChange: (local: string, remote: string) => void;
}

/** Folders that lead to a mapped folder start expanded. */
function leadsToMapping(path: string, mappings: PathMapping[]): boolean {
  return mappings.some((m) => m.local !== path && (path === "" ? m.local !== "" : m.local.startsWith(`${path}/`)));
}

function Row(props: { name: string; path: string; depth: number; root?: boolean } & MappingsTreeProps) {
  const [open, setOpen] = createSignal(!!props.root || leadsToMapping(props.path, props.mappings));
  const [children] = createResource(
    () => (open() ? props.path : null),
    async (path) => {
      const entries = await workspaceApi.listDir(path).catch(() => []);
      return entries.filter((e) => e.kind === "dir").sort((a, b) => a.name.localeCompare(b.name));
    },
  );
  const remote = () => props.mappings.find((m) => m.local === props.path)?.remote ?? "";
  return (
    <>
      <div class="dbg-mapping-row" classList={{ mapped: !!remote() }} data-mapping={props.path || "."}>
        <div class="dbg-mapping-file" style={{ "padding-left": `${props.depth * 16}px` }}>
          <span class="tree-chevron" onClick={() => setOpen((o) => !o)}>
            {open() ? <ChevronDown size={13} /> : <ChevronRight size={13} />}
          </span>
          {props.root ? <FolderRoot size={13} class="muted" /> : <Folder size={13} class="muted" />}
          <span class="dbg-mapping-name" title={props.path || props.name}>
            {props.name}
          </span>
        </div>
        <input
          class="dbg-input dbg-mono"
          value={remote()}
          placeholder={props.root ? "/var/www/html" : " "}
          disabled={props.disabled}
          aria-label={`Absolute path on the server for ${props.path || "the project root"}`}
          onChange={(e) => props.onChange(props.path, e.currentTarget.value.trim())}
        />
      </div>
      <Show when={open()}>
        <For each={children() ?? []}>
          {(c) => (
            <Row
              name={c.name}
              path={props.path ? `${props.path}/${c.name}` : c.name}
              depth={props.depth + 1}
              mappings={props.mappings}
              disabled={props.disabled}
              onChange={props.onChange}
            />
          )}
        </For>
      </Show>
    </>
  );
}

export function MappingsTree(props: MappingsTreeProps) {
  const rootName = () => {
    const root = workspace()?.root ?? "";
    const name = root.replace(/[\\/]+$/, "").split(/[\\/]/).pop();
    return name ? `Project files (${name})` : "Project files";
  };
  return (
    <div class="dbg-mappings" classList={{ disabled: !!props.disabled }}>
      <div class="dbg-mappings-head">
        <span>File/Directory</span>
        <span>Absolute path on the server</span>
      </div>
      <div class="dbg-mappings-body">
        <Row name={rootName()} path="" depth={0} root {...props} />
      </div>
    </div>
  );
}
