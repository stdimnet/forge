import { createEffect, createMemo, createSignal, For, onCleanup, Show } from "solid-js";
import { CaseSensitive, ChevronDown, ChevronRight, ChevronsDownUp, ChevronsUpDown, Regex, RefreshCw, WholeWord } from "lucide-solid";
import type { FileMatches, LineMatch } from "@/ipc/find";
import { openFileAt } from "@/editor/views";
import { FileIcon } from "@/shell/FileIcon";
import { fileRows, type MatchRows } from "./contextRows";
import { formatDuration } from "./duration";
import { collapsedFiles, findForm, findResults, hasMoreRows, loadMoreRows, runFindWindow, setCollapsedFiles, setFindForm } from "./window";

const ROW_HEIGHT = 24;
const OVERSCAN = 12;

type Row = { kind: "file"; file: FileMatches } | MatchRows;

const toggleFile = (path: string) =>
  setCollapsedFiles((prev) => {
    const next = new Set(prev);
    if (!next.delete(path)) next.add(path);
    return next;
  });

function MatchText(props: { match: LineMatch }) {
  const parts = createMemo(() => {
    const out: { t: string; hit: boolean }[] = [];
    let at = 0;
    for (const [s, e] of props.match.ranges) {
      if (s > at) out.push({ t: props.match.text.slice(at, s), hit: false });
      out.push({ t: props.match.text.slice(s, e), hit: true });
      at = e;
    }
    if (at < props.match.text.length) out.push({ t: props.match.text.slice(at), hit: false });
    return out;
  });
  return (
    <span class="fw-text">
      <For each={parts()}>{(p) => (p.hit ? <mark>{p.t}</mark> : p.t)}</For>
    </span>
  );
}

function Toggle(props: { title: string; active?: boolean; onClick: () => void; children: unknown }) {
  return (
    <button class="icon-btn" classList={{ active: props.active }} title={props.title} onClick={props.onClick}>
      {props.children as never}
    </button>
  );
}

function FileRow(props: { file: FileMatches }) {
  const name = () => props.file.path.split("/").pop() ?? props.file.path;
  const dir = () => props.file.path.slice(0, props.file.path.length - name().length).replace(/\/$/, "");
  const open = () => !collapsedFiles().has(props.file.path);
  return (
    <div class="tree-row fw-row fw-file" onClick={() => toggleFile(props.file.path)}>
      <span class="tree-chevron">{open() ? <ChevronDown size={14} /> : <ChevronRight size={14} />}</span>
      <FileIcon name={name()} size={14} />
      <span class="tree-label strong">{name()}</span>
      <span class="tree-hint">{dir()}</span>
      <span class="fw-count">{props.file.matches.length}</span>
    </div>
  );
}

function MatchRow(props: { file: FileMatches; match: LineMatch }) {
  return (
    <div
      class="tree-row fw-row fw-match"
      onClick={() => void openFileAt(props.file.path, props.match.line + 1, props.match.column + 1)}
    >
      <span class="fw-line">{props.match.line + 1}</span>
      <MatchText match={props.match} />
    </div>
  );
}

function ContextRow(props: { file: FileMatches; line: number; text: string }) {
  return (
    <div class="tree-row fw-row fw-context" onClick={() => void openFileAt(props.file.path, props.line + 1, 1)}>
      <span class="fw-line">{props.line + 1}</span>
      <span class="fw-text">{props.text}</span>
    </div>
  );
}

export function FindWindow() {
  const [scrollTop, setScrollTop] = createSignal(0);
  const [viewHeight, setViewHeight] = createSignal(300);

  // The results box only exists once a search has run, so observe it when it is created.
  const observe = (el: HTMLDivElement) => {
    setViewHeight(el.clientHeight);
    const observer = new ResizeObserver(() => setViewHeight(el.clientHeight));
    observer.observe(el);
    onCleanup(() => observer.disconnect());
  };

  const files = () => findResults().files;
  const rows = createMemo<Row[]>(() => {
    const folded = collapsedFiles();
    const out: Row[] = [];
    for (const file of files()) {
      out.push({ kind: "file", file });
      if (!folded.has(file.path)) out.push(...fileRows(file));
    }
    return out;
  });
  // Fetch the next page while the list is scrolled near its end, or does not fill the view yet.
  createEffect(() => {
    const end = rows().length * ROW_HEIGHT;
    if (hasMoreRows() && scrollTop() + viewHeight() > end - 40 * ROW_HEIGHT) void loadMoreRows();
  });
  const first = () => Math.max(0, Math.floor(scrollTop() / ROW_HEIGHT) - OVERSCAN);
  const last = () => Math.min(rows().length, Math.ceil((scrollTop() + viewHeight()) / ROW_HEIGHT) + OVERSCAN);

  const foldAll = (fold: boolean) => setCollapsedFiles(fold ? new Set(files().map((f) => f.path)) : new Set<string>());
  const flip = (key: "caseSensitive" | "wholeWords" | "regex") => () => {
    setFindForm(key, (v) => !v);
    void runFindWindow();
  };
  const onEnter = (e: KeyboardEvent) => e.key === "Enter" && void runFindWindow();

  return (
    <div class="fw-view">
      <div class="fw-toolbar">
        <input
          class="fw-pattern"
          placeholder="Text to find"
          value={findForm.query}
          spellcheck={false}
          onInput={(e) => setFindForm("query", e.currentTarget.value)}
          onKeyDown={onEnter}
        />
        <Toggle title="Match case" active={findForm.caseSensitive} onClick={flip("caseSensitive")}>
          <CaseSensitive size={15} />
        </Toggle>
        <Toggle title="Words" active={findForm.wholeWords} onClick={flip("wholeWords")}>
          <WholeWord size={15} />
        </Toggle>
        <Toggle title="Regex" active={findForm.regex} onClick={flip("regex")}>
          <Regex size={15} />
        </Toggle>
        <input
          class="fw-mask"
          placeholder="File mask: *.php, !*.min.js"
          value={findForm.fileMask}
          spellcheck={false}
          onInput={(e) => setFindForm("fileMask", e.currentTarget.value)}
          onKeyDown={onEnter}
        />
        <input
          class="fw-dir"
          placeholder="Directory"
          value={findForm.directory}
          spellcheck={false}
          onInput={(e) => setFindForm("directory", e.currentTarget.value)}
          onKeyDown={onEnter}
        />
        <select
          class="fw-context-select"
          title="Lines of context"
          value={String(findForm.context)}
          onChange={(e) => {
            setFindForm("context", Number(e.currentTarget.value));
            void runFindWindow();
          }}
        >
          <For each={[0, 1, 2, 3, 5]}>{(n) => <option value={String(n)}>{n === 0 ? "No context" : `${n} line${n > 1 ? "s" : ""} of context`}</option>}</For>
        </select>
        <Toggle title="Search again" onClick={() => void runFindWindow()}>
          <RefreshCw size={14} />
        </Toggle>
        <Toggle title="Expand all" onClick={() => foldAll(false)}>
          <ChevronsUpDown size={14} />
        </Toggle>
        <Toggle title="Collapse all" onClick={() => foldAll(true)}>
          <ChevronsDownUp size={14} />
        </Toggle>
      </div>
      <Show
        when={findResults().searched}
        fallback={<div class="placeholder-panel">Search with Ctrl+Shift+F, then press "Open in Find Window" to list every match here.</div>}
      >
        <div class="fw-summary">
          <Show when={!findResults().busy} fallback={<span>Searching…</span>}>
            <span>
              {findResults().matchCount}
              {findResults().truncated ? "+" : ""} matches in {findResults().fileCount} files in {formatDuration(findResults().tookMs)}
            </span>
          </Show>
          <Show when={findResults().indexing}>
            <span class="fw-note">Indexing, results may be incomplete</span>
          </Show>
          <Show when={findResults().truncated}>
            <span class="fw-note">Stopped at the match limit (Settings, Tools, Search)</span>
          </Show>
        </div>
        <Show when={findResults().error}>
          <div class="fw-error">{findResults().error}</div>
        </Show>
        <div class="fw-results" ref={observe} onScroll={(e) => setScrollTop(e.currentTarget.scrollTop)}>
          <div style={{ height: `${rows().length * ROW_HEIGHT}px`, position: "relative" }}>
            <div style={{ transform: `translateY(${first() * ROW_HEIGHT}px)` }}>
              <For each={rows().slice(first(), last())}>
                {(row) =>
                  row.kind === "file" ? (
                    <FileRow file={row.file} />
                  ) : row.kind === "context" ? (
                    <ContextRow file={row.file} line={row.line} text={row.text} />
                  ) : (
                    <MatchRow file={row.file} match={row.match} />
                  )
                }
              </For>
            </div>
          </div>
          <Show when={!findResults().busy && !findResults().error && files().length === 0}>
            <div class="placeholder-panel">Nothing found.</div>
          </Show>
        </div>
      </Show>
    </div>
  );
}
