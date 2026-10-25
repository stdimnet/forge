/**
 * Compare Directories: the folder diff in an editor tab, like PHPStorm's directory diff.
 * Left and right columns with size and date, an indicator per row (equal, different, only
 * on the left, only on the right), filters for each state, file diffs on double-click and
 * copy actions in both directions.
 */
import { createMemo, createResource, createSignal, For, Show } from "solid-js";
import {
  ArrowLeft,
  ArrowLeftToLine,
  ArrowRight,
  ArrowRightToLine,
  ChevronDown,
  ChevronRight,
  Columns2,
  Equal,
  EqualNot,
  RefreshCw,
} from "lucide-solid";
import { FileIcon } from "@/shell/FileIcon";
import { askConfirm } from "@/features/contextMenu/dialogs";
import { refreshDirs } from "@/features/project/ProjectView";
import { reloadOpenFiles, repositoryChanged } from "@/features/git/store";
import { setDiffRequest } from "@/features/git/overlays";
import { fullDate } from "@/features/git/format";
import { showBalloon } from "@/keymap";
import { compareApi, type DiffStatus, type DirCompare, type DirDiffEntry } from "./api";
import { compareFiles, readText } from "./state";

const join = (root: string, rel: string) => (root ? `${root}/${rel}` : rel);
const parentOf = (path: string) => path.split("/").slice(0, -1).join("/");

/** Compare Contents of the folder diff that last had the focus (Ctrl+D). */
let focusedDiff: (() => void) | null = null;
export const compareFocusedDirDiffRow = () => focusedDiff?.();

export function formatSize(bytes: number | null): string {
  if (bytes === null) return "";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

const STATES: { status: DiffStatus; title: string; icon: typeof Equal }[] = [
  { status: "leftOnly", title: "Show Files Only on the Left", icon: ArrowRight },
  { status: "different", title: "Show Different Files", icon: EqualNot },
  { status: "equal", title: "Show Equal Files", icon: Equal },
  { status: "rightOnly", title: "Show Files Only on the Right", icon: ArrowLeft },
];

const STATUS_TITLE: Record<DiffStatus, string> = {
  equal: "Equal",
  different: "Different",
  leftOnly: "Only on the left",
  rightOnly: "Only on the right",
};

/** Rows to show: entries passing the filters, folders holding such entries, nothing below a collapsed folder. */
export function visibleRows(entries: DirDiffEntry[], show: Record<DiffStatus, boolean>, collapsed: Set<string>): DirDiffEntry[] {
  const keep = new Set<string>();
  for (const e of entries) {
    if (!show[e.status] || (e.dir && e.status === "different")) continue;
    keep.add(e.path);
    let p = parentOf(e.path);
    while (p) {
      keep.add(p);
      p = parentOf(p);
    }
  }
  const hidden = (path: string) => {
    let p = parentOf(path);
    while (p) {
      if (collapsed.has(p)) return true;
      p = parentOf(p);
    }
    return false;
  };
  return entries.filter((e) => keep.has(e.path) && !hidden(e.path));
}

/**
 * Where a folder diff comes from when it is not two project folders (Sync with Deployed:
 * the project on the left, a deployment server on the right).
 */
export interface DirDiffSource {
  load: () => Promise<DirCompare>;
  /** Shown above each side instead of the folder paths. */
  leftTitle: string;
  rightTitle: string;
  /** Compare Contents of a file row. */
  openDiff: (e: DirDiffEntry) => void | Promise<void>;
  /** Copy a row to the other side (Upload / Download). */
  copy: (e: DirDiffEntry, toRight: boolean) => Promise<void>;
  /** Button titles, e.g. "Upload to shop-production" / "Download from shop-production". */
  copyTitles: { toRight: string; toLeft: string };
}

export function DirDiffView(props: { left: string; right: string; source?: DirDiffSource }) {
  const [data, { refetch }] = createResource(() => (props.source ? props.source.load() : compareApi.dirs(props.left, props.right)));
  const [show, setShow] = createSignal<Record<DiffStatus, boolean>>({ leftOnly: true, different: true, equal: true, rightOnly: true });
  const [collapsed, setCollapsed] = createSignal(new Set<string>());
  const [selected, setSelected] = createSignal<string | null>(null);
  let list!: HTMLDivElement;

  const entries = () => data()?.entries ?? [];
  const rows = createMemo(() => visibleRows(entries(), show(), collapsed()));
  const counts = createMemo(() => {
    const c: Record<DiffStatus, number> = { equal: 0, different: 0, leftOnly: 0, rightOnly: 0 };
    for (const e of entries()) if (!e.dir) c[e.status]++;
    return c;
  });
  const current = () => entries().find((e) => e.path === selected());

  const toggleDir = (path: string) =>
    setCollapsed((prev) => {
      const next = new Set(prev);
      next.has(path) ? next.delete(path) : next.add(path);
      return next;
    });

  /** Compare Contents: the file diff of a row (an empty side for files on one side only). */
  const openDiff = async (e: DirDiffEntry | undefined) => {
    if (!e) return;
    if (e.dir) return toggleDir(e.path);
    if (props.source) return props.source.openDiff(e);
    const left = join(props.left, e.path);
    const right = join(props.right, e.path);
    if (e.status !== "leftOnly" && e.status !== "rightOnly") return compareFiles(left, right);
    const text = (await readText(e.status === "leftOnly" ? left : right)) ?? "";
    const empty = { title: "Does not exist", content: "" };
    setDiffRequest({
      path: e.status === "leftOnly" ? left : right,
      left: e.status === "leftOnly" ? { title: left, content: text } : empty,
      right: e.status === "rightOnly" ? { title: right, content: text } : empty,
      editable: false,
    });
  };

  /** Copy to Right / Copy to Left: the selected entry replaces the other side. */
  const copy = async (toRight: boolean) => {
    const e = current();
    if (!e || e.status === (toRight ? "rightOnly" : "leftOnly") || e.status === "equal") return;
    if (props.source) {
      try {
        await props.source.copy(e, toRight);
      } catch (err) {
        return showBalloon(toRight ? props.source.copyTitles.toRight : props.source.copyTitles.toLeft, String(err instanceof Error ? err.message : err));
      }
      await refetch();
      return;
    }
    const from = join(toRight ? props.left : props.right, e.path);
    const to = join(toRight ? props.right : props.left, e.path);
    const replaces = e.status === "different";
    if (replaces) {
      const ok = await askConfirm({
        title: toRight ? "Copy to Right" : "Copy to Left",
        message: `Replace ${to} with ${from}?`,
        confirm: "Replace",
      });
      if (!ok) return;
    }
    try {
      await compareApi.copy(from, to, e.dir);
    } catch (err) {
      return showBalloon("Copy failed", String(err instanceof Error ? err.message : err));
    }
    refreshDirs([parentOf(to), to]);
    await reloadOpenFiles();
    void repositoryChanged();
    await refetch();
  };

  const onKeyDown = (e: KeyboardEvent) => {
    const list = rows();
    const at = list.findIndex((r) => r.path === selected());
    let next: DirDiffEntry | undefined;
    if (e.key === "ArrowDown") next = list[Math.min(list.length - 1, at + 1)];
    else if (e.key === "ArrowUp") next = list[Math.max(0, at - 1)];
    else if (e.key === "Enter") void openDiff(current());
    else return;
    e.preventDefault();
    if (next) {
      setSelected(next.path);
      listRow(next.path)?.scrollIntoView({ block: "nearest" });
    }
  };
  const listRow = (path: string) => list.querySelector(`[data-path="${CSS.escape(path)}"]`);

  const canCopy = (toRight: boolean) => {
    const e = current();
    return !!e && e.status !== "equal" && e.status !== (toRight ? "rightOnly" : "leftOnly");
  };

  const Side = (p: { e: DirDiffEntry; left: boolean }) => {
    const present = () => (p.left ? p.e.status !== "rightOnly" : p.e.status !== "leftOnly");
    const name = p.e.path.split("/").pop() ?? p.e.path;
    return (
      <>
        <div class="dir-diff-name" style={{ "padding-left": `${6 + p.e.depth * 16}px` }}>
          <span
            class="tree-chevron"
            onClick={(ev) => {
              if (!p.e.dir) return;
              ev.stopPropagation();
              toggleDir(p.e.path);
            }}
          >
            <Show when={p.e.dir && present()}>{collapsed().has(p.e.path) ? <ChevronRight size={14} /> : <ChevronDown size={14} />}</Show>
          </span>
          <Show when={present()}>
            <FileIcon name={name} dir={p.e.dir} open={p.e.dir && !collapsed().has(p.e.path)} />
            <span class={`tree-label dir-diff-${p.e.status}`}>{name}</span>
          </Show>
        </div>
        <div class="dir-diff-size">{present() ? formatSize(p.left ? p.e.leftSize : p.e.rightSize) : ""}</div>
        <div class="dir-diff-date">
          {present() ? ((m) => (m ? fullDate(m) : ""))(p.left ? p.e.leftModified : p.e.rightModified) : ""}
        </div>
      </>
    );
  };

  return (
    <div class="dir-diff" onFocusIn={() => (focusedDiff = () => void openDiff(current()))}>
      <div class="panel-toolbar dir-diff-toolbar">
        <button class="icon-btn" title="Refresh" onClick={() => void refetch()}>
          <RefreshCw size={15} />
        </button>
        <button class="icon-btn" title="Compare Contents (Ctrl+D)" disabled={!current() || current()!.dir} onClick={() => void openDiff(current())}>
          <Columns2 size={15} />
        </button>
        <div class="toolbar-sep" />
        <button class="icon-btn" title={props.source?.copyTitles.toRight ?? "Copy to Right"} disabled={!canCopy(true)} onClick={() => void copy(true)}>
          <ArrowRightToLine size={15} />
        </button>
        <button class="icon-btn" title={props.source?.copyTitles.toLeft ?? "Copy to Left"} disabled={!canCopy(false)} onClick={() => void copy(false)}>
          <ArrowLeftToLine size={15} />
        </button>
        <div class="toolbar-sep" />
        <For each={STATES}>
          {(s) => (
            <button
              class="pill-btn dir-diff-filter"
              classList={{ active: show()[s.status] }}
              title={s.title}
              aria-pressed={show()[s.status]}
              onClick={() => setShow((prev) => ({ ...prev, [s.status]: !prev[s.status] }))}
            >
              <span class={`dir-diff-indicator dir-diff-${s.status}`}>
                <s.icon size={13} />
              </span>
              {counts()[s.status]}
            </button>
          )}
        </For>
        <span class="spacer" />
        <span class="diff-count">
          <Show when={data()} fallback={data.error ? "Comparison failed" : "Comparing…"}>
            {counts().different + counts().leftOnly + counts().rightOnly === 0
              ? "No differences"
              : `${counts().different} different, ${counts().leftOnly} only on the left, ${counts().rightOnly} only on the right`}
            {data()?.truncated ? " (too many files, partially compared)" : ""}
          </Show>
        </span>
      </div>
      <div class="dir-diff-header">
        <div class="dir-diff-root" title={props.source?.leftTitle ?? props.left}>
          {props.source?.leftTitle ?? (props.left || "/")}
        </div>
        <div />
        <div class="dir-diff-root" title={props.source?.rightTitle ?? props.right}>
          {props.source?.rightTitle ?? (props.right || "/")}
        </div>
      </div>
      <div class="dir-diff-columns">
        <div>Name</div>
        <div>Size</div>
        <div>Date</div>
        <div />
        <div>Name</div>
        <div>Size</div>
        <div>Date</div>
      </div>
      <div class="dir-diff-list" tabIndex={0} ref={list} onKeyDown={onKeyDown}>
        <Show when={data.error}>
          <div class="commit-empty">{String(data.error instanceof Error ? data.error.message : data.error)}</div>
        </Show>
        <For each={rows()}>
          {(e) => {
            const Icon = STATES.find((s) => s.status === e.status)!.icon;
            return (
              <div
                class="dir-diff-row"
                classList={{ selected: selected() === e.path }}
                data-path={e.path}
                onClick={() => setSelected(e.path)}
                onDblClick={() => void openDiff(e)}
              >
                <Side e={e} left />
                <div class={`dir-diff-indicator dir-diff-${e.status}`} title={STATUS_TITLE[e.status]}>
                  <Icon size={13} />
                </div>
                <Side e={e} left={false} />
              </div>
            );
          }}
        </For>
        <Show when={data() && !rows().length}>
          <div class="commit-empty">{entries().length ? "No files match the filters" : "Both folders are empty"}</div>
        </Show>
      </div>
    </div>
  );
}
