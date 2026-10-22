/**
 * PHPStorm's "Conflicts" dialog: the unmerged files with how each side changed them, Accept
 * Yours / Accept Theirs / Merge… and "Group files by directory".
 */
import { createEffect, createMemo, createSignal, For, on, Show } from "solid-js";
import { ChevronDown, Folder } from "lucide-solid";
import { FileIcon } from "@/shell/FileIcon";
import { gitApi, type ConflictFile, type ConflictSet, type SideChange } from "./api";
import { acceptSide, closeConflicts, openMerge } from "./conflicts";
import { status } from "./store";

const [grouped, setGrouped] = createSignal(false);

const dirOf = (p: string) => (p.includes("/") ? p.slice(0, p.lastIndexOf("/")) : "");
const nameOf = (p: string) => p.slice(p.lastIndexOf("/") + 1);
const label = (c: SideChange) => (c === "modified" ? "Modified" : c === "added" ? "Added" : "Deleted");

type Row = { kind: "dir"; dir: string } | { kind: "file"; file: ConflictFile };

export function ConflictsDialog() {
  const [set, setSet] = createSignal<ConflictSet | null>(null);
  const [selected, setSelected] = createSignal<Set<string>>(new Set());
  const [anchor, setAnchor] = createSignal<string | null>(null);
  const [busy, setBusy] = createSignal(false);

  const load = async () => {
    const next = await gitApi.conflicts().catch(() => null);
    setSet(next);
    const paths = new Set(next?.files.map((f) => f.path) ?? []);
    const keep = new Set([...selected()].filter((p) => paths.has(p)));
    if (!keep.size && next?.files.length) keep.add(order()[0] ?? next.files[0].path);
    setSelected(keep);
  };
  // Reload whenever the repository status changes (a file was resolved elsewhere too).
  createEffect(on(status, () => void load()));

  const files = () => [...(set()?.files ?? [])].sort((a, b) => a.path.localeCompare(b.path));
  const rows = createMemo<Row[]>(() => {
    if (!grouped()) return files().map((file) => ({ kind: "file", file }));
    const out: Row[] = [];
    let dir: string | null = null;
    for (const file of [...files()].sort((a, b) => dirOf(a.path).localeCompare(dirOf(b.path)) || a.path.localeCompare(b.path))) {
      if (dirOf(file.path) !== dir) {
        dir = dirOf(file.path);
        out.push({ kind: "dir", dir });
      }
      out.push({ kind: "file", file });
    }
    return out;
  });
  const order = () => rows().flatMap((r) => (r.kind === "file" ? [r.file.path] : []));
  const selection = () => order().filter((p) => selected().has(p));

  const click = (e: MouseEvent, path: string) => {
    if (e.shiftKey && anchor()) {
      const list = order();
      const [a, b] = [list.indexOf(anchor()!), list.indexOf(path)].sort((x, y) => x - y);
      setSelected(new Set(list.slice(a, b + 1)));
      return;
    }
    if (e.ctrlKey || e.metaKey) {
      const next = new Set(selected());
      if (next.has(path)) next.delete(path);
      else next.add(path);
      setSelected(next);
    } else setSelected(new Set([path]));
    setAnchor(path);
  };

  const run = async (op: () => Promise<unknown>) => {
    if (busy()) return;
    setBusy(true);
    try {
      await op();
    } finally {
      setBusy(false);
    }
  };
  const accept = (side: "yours" | "theirs") => run(() => acceptSide(selection(), side, set()?.operation));
  const merge = () => {
    const [path] = selection();
    if (path) void run(() => openMerge(path));
  };

  const move = (d: number) => {
    const list = order();
    const at = list.indexOf(selection()[0] ?? anchor() ?? "");
    const next = list[Math.min(list.length - 1, Math.max(0, at + d))];
    if (next) {
      setSelected(new Set([next]));
      setAnchor(next);
    }
  };

  return (
    <div class="modal-backdrop" onMouseDown={(e) => e.target === e.currentTarget && closeConflicts()}>
      <div
        class="git-dialog conflicts-dialog"
        role="dialog"
        aria-label="Conflicts"
        onKeyDown={(e) => {
          if (e.key === "Escape") closeConflicts();
          else return;
          e.preventDefault();
        }}
      >
        <header class="dialog-title">
          <span>Conflicts</span>
        </header>
        <div class="conflicts-body">
          <div class="conflicts-description">{set()?.description ?? "Loading conflicts…"}</div>
          <div class="conflicts-main">
            <div
              class="conflicts-table"
              role="grid"
              tabIndex={0}
              ref={(el) => queueMicrotask(() => el.focus())}
              onKeyDown={(e) => {
                if (e.key === "ArrowDown") move(1);
                else if (e.key === "ArrowUp") move(-1);
                else if (e.key === "Enter") merge();
                else return;
                e.preventDefault();
              }}
            >
              <div class="conflicts-row conflicts-head">
                <span>Name</span>
                <span>Yours{set() ? ` (${set()!.yoursName})` : ""}</span>
                <span>Theirs{set() ? ` (${set()!.theirsName})` : ""}</span>
              </div>
              <For each={rows()}>
                {(row) =>
                  row.kind === "dir" ? (
                    <div class="conflicts-row conflicts-dir">
                      <span>
                        <ChevronDown size={13} class="muted" />
                        <Folder size={14} stroke-width={1.75} class="muted" />
                        {row.dir || "(project root)"}
                      </span>
                      <span />
                      <span />
                    </div>
                  ) : (
                    <div
                      class="conflicts-row conflicts-file"
                      classList={{ selected: selected().has(row.file.path), indented: grouped() }}
                      data-path={row.file.path}
                      onClick={(e) => click(e, row.file.path)}
                      onDblClick={() => {
                        setSelected(new Set([row.file.path]));
                        merge();
                      }}
                      title={row.file.path}
                    >
                      <span>
                        <FileIcon name={nameOf(row.file.path)} />
                        <span class="vcs-conflict">{nameOf(row.file.path)}</span>
                        <Show when={!grouped() && dirOf(row.file.path)}>
                          <span class="tree-hint">{dirOf(row.file.path)}</span>
                        </Show>
                      </span>
                      <span>{label(row.file.yours)}</span>
                      <span>{label(row.file.theirs)}</span>
                    </div>
                  )
                }
              </For>
              <Show when={set() && !files().length}>
                <div class="conflicts-empty muted">No conflicts</div>
              </Show>
            </div>
            <div class="conflicts-buttons">
              <button class="btn-secondary" disabled={busy() || !selection().length} onClick={() => void accept("yours")}>
                Accept Yours
              </button>
              <button class="btn-secondary" disabled={busy() || !selection().length} onClick={() => void accept("theirs")}>
                Accept Theirs
              </button>
              <button class="btn-primary" disabled={busy() || selection().length !== 1} onClick={merge}>
                Merge…
              </button>
            </div>
          </div>
        </div>
        <footer class="git-dialog-actions">
          <label class="git-dialog-check conflicts-group">
            <input type="checkbox" checked={grouped()} onChange={(e) => setGrouped(e.currentTarget.checked)} />
            Group files by directory
          </label>
          <span class="spacer" />
          <button class="btn-secondary" onClick={closeConflicts}>
            Close
          </button>
        </footer>
      </div>
    </div>
  );
}
