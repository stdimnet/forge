/**
 * PHPStorm's Find in Files popup (Ctrl+Shift+F): query with Match case / Words / Regex,
 * file mask, In Project / Directory scope, the matching lines and a preview of the selected
 * one. The search itself runs in Rust (`find_in_files`), through the project's backend.
 */
import { createEffect, createMemo, createSignal, For, on, onCleanup, onMount, Show } from "solid-js";
import { EditorState, RangeSetBuilder, StateEffect, StateField } from "@codemirror/state";
import { Decoration, EditorView, lineNumbers, type DecorationSet } from "@codemirror/view";
import { PanelBottomOpen, Search } from "lucide-solid";
import { findApi, type FindResult, type LineMatch } from "@/ipc/find";
import { workspaceApi } from "@/ipc/workspace";
import { openFileAt } from "@/editor/views";
import { forgeEditorTheme, forgeHighlight } from "@/editor/forgeTheme";
import { languageFor } from "@/editor/languages";
import { ensureFileType } from "@/editor/fileTypes";
import { editor } from "@/state/editor";
import { FileIcon } from "@/shell/FileIcon";
import { formatDuration } from "./duration";
import { scopeRequest } from "./scope";
import { ScopeSelect } from "@/features/scopes/ScopeSelect";
import { openFindWindow } from "./window";
import { closeFindInFiles, findOptions, findQuery, findRequest, openFindResultsInWindow, setFindQuery, updateFindOptions } from "./state";
import { t } from "@/i18n";
import { latestOnly } from "@/state/async";

interface Row {
  path: string;
  match: LineMatch;
}

const fileName = (path: string) => path.split("/").pop() ?? path;

/** Highlighted segments of a result line. */
function Highlighted(props: { text: string; ranges: [number, number][] }) {
  const parts = createMemo(() => {
    const out: { t: string; hit: boolean }[] = [];
    let at = 0;
    for (const [s, e] of props.ranges) {
      if (s > at) out.push({ t: props.text.slice(at, s), hit: false });
      out.push({ t: props.text.slice(s, e), hit: true });
      at = e;
    }
    if (at < props.text.length) out.push({ t: props.text.slice(at), hit: false });
    return out;
  });
  return <For each={parts()}>{(p) => (p.hit ? <mark class="hl">{p.t}</mark> : p.t)}</For>;
}

const setMatches = StateEffect.define<DecorationSet>();
const matchField = StateField.define<DecorationSet>({
  create: () => Decoration.none,
  update: (deco, tr) => {
    for (const e of tr.effects) if (e.is(setMatches)) return e.value;
    return deco;
  },
  provide: (f) => EditorView.decorations.from(f),
});

/** Read-only preview of the selected match's file. */
function Preview(props: { row: Row | undefined }) {
  let host!: HTMLDivElement;
  let view: EditorView | undefined;
  let shown: string | undefined;
  const contentOf = async (path: string) => editor.tabs.find((t) => t.path === path)?.content ?? workspaceApi.readFile(path);

  // A slow read of an earlier selection's file must not replace the newer preview.
  const shows = latestOnly();
  const show = async (row: Row) => {
    const current = shows.begin();
    if (shown !== row.path || !view) {
      const content = await contentOf(row.path).catch(() => "");
      await ensureFileType(row.path, content);
      if (!current()) return;
      view?.destroy();
      view = new EditorView({
        parent: host,
        state: EditorState.create({
          doc: content,
          extensions: [
            lineNumbers(),
            EditorState.readOnly.of(true),
            EditorView.editable.of(false),
            forgeEditorTheme,
            forgeHighlight,
            ...languageFor(row.path, content),
            matchField,
          ],
        }),
      });
      shown = row.path;
    }
    const doc = view.state.doc;
    const line = doc.line(Math.min(doc.lines, row.match.line + 1));
    // Ranges are relative to the listed (trimmed) text; the first one is at `column`.
    const shift = row.match.column - (row.match.ranges[0]?.[0] ?? 0);
    const marks = new RangeSetBuilder<Decoration>();
    marks.add(line.from, line.from, Decoration.line({ class: "find-preview-line" }));
    for (const [s, e] of row.match.ranges) {
      const from = Math.min(line.to, line.from + shift + s);
      const to = Math.min(line.to, line.from + shift + e);
      if (to > from) marks.add(from, to, Decoration.mark({ class: "find-preview-match" }));
    }
    view.dispatch({
      effects: [setMatches.of(marks.finish()), EditorView.scrollIntoView(line.from, { y: "center" })],
    });
  };

  createEffect(
    on(
      () => props.row,
      (row) => {
        if (row) void show(row);
        else {
          shows.invalidate();
          view?.destroy();
          view = undefined;
          shown = undefined;
        }
      },
    ),
  );
  onCleanup(() => view?.destroy());
  return (
    <div class="find-preview">
      <div class="find-preview-header">
        <Show when={props.row}>
          {(r) => (
            <>
              <FileIcon name={fileName(r().path)} path={r().path} size={14} />
              <span>{r().path}</span>
            </>
          )}
        </Show>
      </div>
      <div class="find-preview-editor" ref={host} />
    </div>
  );
}

export function FindInFilesPopup() {
  let input!: HTMLInputElement;
  let list!: HTMLDivElement;
  const [result, setResult] = createSignal<FindResult | null>(null);
  const [loading, setLoading] = createSignal(false);
  const [tookMs, setTookMs] = createSignal(0);
  const [selected, setSelected] = createSignal(0);
  let seq = 0;
  let timer: ReturnType<typeof setTimeout> | undefined;

  const rows = createMemo<Row[]>(() =>
    (result()?.files ?? []).flatMap((f) => f.matches.map((match) => ({ path: f.path, match }))),
  );

  const run = async () => {
    const query = findQuery();
    const o = findOptions();
    const id = ++seq;
    const started = performance.now();
    if (!query) {
      setResult(null);
      setLoading(false);
      return;
    }
    setLoading(true);
    try {
      const r = await findApi.search(findRequest(query, o));
      if (id !== seq) return;
      setResult(r);
      setTookMs(performance.now() - started);
      setSelected(0);
    } catch (e) {
      if (id === seq) setResult({ files: [], matchCount: 0, truncated: false, error: String(e) });
    } finally {
      if (id === seq) setLoading(false);
    }
  };

  createEffect(
    on([findQuery, findOptions], () => {
      clearTimeout(timer);
      timer = setTimeout(() => void run(), 150);
    }),
  );
  onCleanup(() => clearTimeout(timer));
  onMount(() => {
    input.focus();
    input.select();
  });

  const open = (row: Row | undefined) => {
    if (!row) return;
    closeFindInFiles();
    void openFileAt(row.path, row.match.line + 1, row.match.column + 1);
  };

  const move = (delta: number) => {
    const n = rows().length;
    if (!n) return;
    setSelected((i) => Math.max(0, Math.min(n - 1, i + delta)));
    list.querySelector(`[data-row="${selected()}"]`)?.scrollIntoView({ block: "nearest" });
  };

  // List navigation inside the popup (the popup owns these keys while it has focus).
  const onKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Escape") closeFindInFiles();
    else if (e.key === "ArrowDown") move(1);
    else if (e.key === "ArrowUp") move(-1);
    else if (e.key === "PageDown") move(10);
    else if (e.key === "PageUp") move(-10);
    else if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) openInWindow();
    else if (e.key === "Enter") open(rows()[selected()]);
    else return;
    e.preventDefault();
    e.stopPropagation();
  };

  const openAllInWindow = () => {
    const o = findOptions();
    closeFindInFiles();
    openFindWindow({
      query: findQuery(),
      caseSensitive: o.caseSensitive,
      wholeWords: o.wholeWords,
      regex: o.regex,
      fileMask: o.maskEnabled ? o.fileMask : "",
      ...scopeRequest(o),
      scopeName: o.scope === "scope" ? o.scopeName : undefined,
    });
  };

  /** "Open in Find Window" (Ctrl+Enter): the results become a tab of the Find tool window. */
  const openInWindow = () => {
    const r = result();
    if (!r || r.error || !r.matchCount || !findQuery()) return;
    closeFindInFiles();
    openFindResultsInWindow(findQuery(), findOptions(), r);
  };

  const summary = () => {
    const r = result();
    if (!r || !findQuery()) return "";
    if (r.error) return r.error;
    if (!r.matchCount) return t("find.noMatches", { duration: formatDuration(tookMs()) });
    const plus = r.truncated ? "+" : "";
    const files = r.files.length;
    return t("find.summary", { count: r.matchCount, plus, files, duration: formatDuration(tookMs()) });
  };

  const toggle = (key: "caseSensitive" | "wholeWords" | "regex", label: string, title: string) => (
    <button
      class="find-toggle"
      classList={{ on: findOptions()[key] }}
      title={title}
      aria-pressed={findOptions()[key]}
      onMouseDown={(e) => e.preventDefault()}
      onClick={() => updateFindOptions({ [key]: !findOptions()[key] })}
    >
      {label}
    </button>
  );

  return (
    <div class="popup-backdrop" onMouseDown={(e) => e.target === e.currentTarget && closeFindInFiles()}>
      <div class="popup find-popup" role="dialog" aria-label={t("find.title")} onKeyDown={onKeyDown}>
        <div class="find-header">
          <span class="find-title">{t("find.title")}</span>
          <span class="find-summary" classList={{ error: !!result()?.error }}>
            {summary()}
          </span>
          <label class="find-mask" onMouseDown={(e) => e.stopPropagation()}>
            <input
              type="checkbox"
              checked={findOptions().maskEnabled}
              onChange={(e) => updateFindOptions({ maskEnabled: e.currentTarget.checked })}
            />
            <span>{t("find.fileMask")}</span>
            <input
              class="find-mask-input"
              value={findOptions().fileMask}
              spellcheck={false}
              onInput={(e) => updateFindOptions({ fileMask: e.currentTarget.value, maskEnabled: true })}
            />
          </label>
        </div>
        <div class="find-search">
          <Search size={16} class="muted" />
          <input
            ref={input}
            class="find-input"
            value={findQuery()}
            spellcheck={false}
            autocomplete="off"
            aria-label={t("find.textToFind")}
            onInput={(e) => setFindQuery(e.currentTarget.value)}
          />
          <Show when={loading()}>
            <span class="se-spinner" />
          </Show>
          {toggle("caseSensitive", "Cc", t("find.matchCase"))}
          {toggle("wholeWords", "W", t("find.words"))}
          {toggle("regex", ".*", t("find.regex"))}
          <button
            class="find-toggle"
            title={t("find.openAllInWindow")}
            disabled={!findQuery()}
            onMouseDown={(e) => e.preventDefault()}
            onClick={openAllInWindow}
          >
            <PanelBottomOpen size={14} />
          </button>
        </div>
        <div class="find-scope" role="tablist">
          <button
            class="find-scope-tab"
            classList={{ active: findOptions().scope === "project" }}
            role="tab"
            onMouseDown={(e) => e.preventDefault()}
            onClick={() => updateFindOptions({ scope: "project" })}
          >
            {t("find.scope.project")}
          </button>
          <button
            class="find-scope-tab"
            classList={{ active: findOptions().scope === "directory" }}
            role="tab"
            onMouseDown={(e) => e.preventDefault()}
            onClick={() => updateFindOptions({ scope: "directory" })}
          >
            {t("find.scope.directory")}
          </button>
          <button
            class="find-scope-tab"
            classList={{ active: findOptions().scope === "openFiles" }}
            role="tab"
            onMouseDown={(e) => e.preventDefault()}
            onClick={() => updateFindOptions({ scope: "openFiles" })}
          >
            {t("find.scope.openFiles")}
          </button>
          <button
            class="find-scope-tab"
            classList={{ active: findOptions().scope === "changedFiles" }}
            role="tab"
            onMouseDown={(e) => e.preventDefault()}
            onClick={() => updateFindOptions({ scope: "changedFiles" })}
          >
            {t("find.scope.changedFiles")}
          </button>
          <button
            class="find-scope-tab"
            classList={{ active: findOptions().scope === "scope" }}
            role="tab"
            onMouseDown={(e) => e.preventDefault()}
            onClick={() => updateFindOptions({ scope: "scope" })}
          >
            {t("find.scope.scope")}
          </button>
          <Show when={findOptions().scope === "scope"}>
            <ScopeSelect class="find-scope-select" value={findOptions().scopeName} onChange={(scopeName) => updateFindOptions({ scopeName })} />
          </Show>
          <Show when={findOptions().scope === "directory"}>
            <input
              class="find-dir-input"
              value={findOptions().directory}
              placeholder={t("find.projectRoot")}
              spellcheck={false}
              aria-label={t("find.scope.directory")}
              onInput={(e) => updateFindOptions({ directory: e.currentTarget.value })}
            />
          </Show>
        </div>
        <div class="find-results" ref={list}>
          <Show
            when={rows().length}
            fallback={<div class="se-empty">{findQuery() ? (loading() ? t("find.searching") : summary()) : t("find.typeToSearch")}</div>}
          >
            <For each={rows()}>
              {(row, i) => (
                <div
                  class="find-row"
                  classList={{ selected: i() === selected() }}
                  data-row={i()}
                  onMouseDown={() => setSelected(i())}
                  onDblClick={() => open(row)}
                >
                  <span class="find-row-text">
                    <Highlighted text={row.match.text} ranges={row.match.ranges} />
                  </span>
                  <span class="find-row-file">
                    {fileName(row.path)} {row.match.line + 1}
                  </span>
                </div>
              )}
            </For>
          </Show>
        </div>
        <Preview row={rows()[selected()]} />
        <div class="find-footer">
          <span class="muted">{t("find.footer")}</span>
          <button
            class="btn-secondary find-open-window"
            disabled={!result()?.matchCount}
            onMouseDown={(e) => e.preventDefault()}
            onClick={openInWindow}
          >
            {t("find.openInWindow")}
          </button>
        </div>
      </div>
    </div>
  );
}
