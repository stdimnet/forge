/**
 * Query console tab: SQL editor with schema completion (Redis commands, MongoDB shell),
 * Execute (Ctrl+Enter) of the statement at the caret or the selection, own or shared
 * session, transaction mode, the read-only / production guard, the data source colour, and
 * the Output / result panes (MongoDB results also as JSON).
 */
import { createEffect, createMemo, createSignal, For, on, onCleanup, onMount, Show } from "solid-js";
import { Compartment, EditorState, StateEffect, StateField } from "@codemirror/state";
import {
  Decoration,
  EditorView,
  drawSelection,
  highlightActiveLine,
  highlightActiveLineGutter,
  highlightSpecialChars,
  keymap,
  lineNumbers,
  type DecorationSet,
} from "@codemirror/view";
import { defaultKeymap, history, historyKeymap, indentWithTab } from "@codemirror/commands";
import { bracketMatching, indentOnInput } from "@codemirror/language";
import { autocompletion, closeBrackets } from "@codemirror/autocomplete";
import { highlightSelectionMatches, searchKeymap } from "@codemirror/search";
import { findBar } from "@/features/find/findBar/FindBar";
import {
  Check,
  ChevronDown,
  ChevronFirst,
  ChevronLeft,
  ChevronRight,
  Braces,
  Database,
  Download,
  Lock,
  Play,
  Square,
  Table2,
  Undo2,
  ScrollText,
  History,
  ListTree,
} from "lucide-solid";
import { forgeEditorTheme, forgeHighlight } from "@/editor/forgeTheme";
import { registerView } from "@/editor/views";
import { updateContent, type EditorTab } from "@/state/editor";
import { getSetting } from "@/state/settings";
import { shortcutLabel, showBalloon } from "@/keymap";
import { askConfirm } from "@/features/contextMenu/dialogs";
import { dbApi, dialectOf, DS_COLORS, supportsTransactions, type Cell, type ColumnMeta, type SessionInfo, type StatementResult } from "@/ipc/db";
import { ResultGrid, ResultTools, type ResultGridTools } from "./ResultGrid";
import { ContextMenu, type MenuItem } from "./ContextMenu";
import { gridColumns } from "./grid";
import { connect, consoleSourceId, defaultSchema, openDataSources, sourceById, sourceColor, states, tableInfo, takeConsoleOptions } from "./store";
import { consoles } from "./controllers";
import { sqlWithSchema } from "./completion";
import { exportMenu } from "./exporting";
import { recordHistory, sqlConsoleExtensions, withParameters } from "@/features/sqlEditor/console";
import { consoleDialect } from "@/features/sqlEditor/language";
import { openHistory } from "@/features/sqlEditor/SqlDialogs";
import { dbSchemaApi, type ExplainResult } from "@/ipc/dbSchema";
import { PlanView } from "./schema/PlanView";
import { t } from "@/i18n";

const errorText = (e: unknown) => (e instanceof Error ? e.message : String(e));

interface LogLine {
  time: string;
  kind: "sql" | "info" | "error";
  text: string;
}

interface ResultTab {
  key: number;
  title: string;
  sql: string;
  columns: ColumnMeta[];
  rows: Cell[][];
  hasMore: boolean;
  offset: number;
  elapsedMs: number;
  table?: string;
  /** MongoDB documents (JSON view). */
  documents?: unknown[];
  /** Explain Plan: the plan view instead of the grid. */
  explain?: ExplainResult;
}

const now = () => {
  const d = new Date();
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`;
};

const rowsText = (n: number) => `${n} row${n === 1 ? "" : "s"}`;

/** Table a simple `SELECT … FROM [schema.]table` reads from. */
export function sourceTable(sqlText: string): string | undefined {
  const m = /\bfrom\s+[`"]?(\w+)[`"]?(?:\s*\.\s*[`"]?(\w+)[`"]?)?/i.exec(sqlText);
  if (!m || /\bjoin\b/i.test(sqlText)) return undefined;
  return m[2] ?? m[1];
}

// Brief highlight of the statement that was just executed.
const flashEffect = StateEffect.define<{ from: number; to: number } | null>();
const flashField = StateField.define<DecorationSet>({
  create: () => Decoration.none,
  update(deco, tr) {
    deco = deco.map(tr.changes);
    for (const e of tr.effects) {
      if (e.is(flashEffect)) deco = e.value && e.value.to > e.value.from ? Decoration.set([Decoration.mark({ class: "cm-db-executed" }).range(e.value.from, e.value.to)]) : Decoration.none;
    }
    return deco;
  },
  provide: (f) => EditorView.decorations.from(f),
});

function ResultView(props: { result: ResultTab; dsId: string; onPage: (offset: number) => void; pageSize: number }) {
  const [json, setJson] = createSignal(false);
  const [menu, setMenu] = createSignal<{ x: number; y: number; items: MenuItem[] } | null>(null);
  const ds = () => sourceById(props.dsId);
  const table = () => (props.result.table ? tableInfo(props.dsId, defaultSchema(props.dsId), props.result.table) : undefined);
  const columns = createMemo(() => gridColumns(props.result.columns, table()));
  // Rows in display order (local sort and filter of the result grid), for export.
  const [rows, setRows] = createSignal<Cell[][]>(props.result.rows);
  const [tools, setTools] = createSignal<ResultGridTools>();
  const first = () => (props.result.rows.length ? props.result.offset + 1 : 0);
  return (
    <div class="db-result">
      <div class="db-toolbar small">
        <button class="icon-btn db-tool" title={t("database.consoleView.firstPage")} disabled={props.result.offset === 0} onClick={() => props.onPage(0)}>
          <ChevronFirst size={15} />
        </button>
        <button class="icon-btn db-tool" title={t("database.consoleView.previousPage")} disabled={props.result.offset === 0} onClick={() => props.onPage(Math.max(0, props.result.offset - props.pageSize))}>
          <ChevronLeft size={15} />
        </button>
        <span class="db-paging">
          {first()}-{props.result.offset + props.result.rows.length}
          <Show when={!props.result.hasMore && props.result.offset === 0}>
            <span class="muted"> of {props.result.rows.length}</span>
          </Show>
        </span>
        <button class="icon-btn db-tool" title={t("database.consoleView.nextPage")} disabled={!props.result.hasMore} onClick={() => props.onPage(props.result.offset + props.pageSize)}>
          <ChevronRight size={15} />
        </button>
        <span class="db-sep" />
        <button
          class="icon-btn db-tool"
          title={t("database.consoleView.exportData")}
          onClick={(e) => {
            const r = e.currentTarget.getBoundingClientRect();
            setMenu({
              x: r.left,
              y: r.bottom + 2,
              items: exportMenu(() => ({
                dialect: dialectOf(ds()?.driver ?? "mysql", ds()?.options.ddlDialect),
                table: props.result.table,
                columns: props.result.columns.map((c) => c.name),
                rows: rows(),
              })),
            });
          }}
        >
          <Download size={14} />
        </button>
        <Show when={props.result.documents?.length}>
          <button class="db-chip-btn" classList={{ open: json() }} title={t("database.consoleView.showTheDocumentsAsJson")} data-testid="db-json-toggle" onClick={() => setJson(!json())}>
            <Braces size={12} /> {json() ? t("database.consoleView.table") : "JSON"}
          </button>
        </Show>
        <Show when={tools() && !json()}>
          <span class="db-sep" />
          <ResultTools tools={tools()!} />
        </Show>
        <span class="db-spacer" />
        <span class="db-status muted">
          {t("tools.database.ms", { value: rowsText(props.result.rows.length), value2: props.result.hasMore ? "+" : "", elapsedMs: props.result.elapsedMs })}
        </span>
      </div>
      <Show
        when={json() && props.result.documents}
        fallback={
          <ResultGrid
            columns={columns()}
            rows={props.result.rows}
            rowOffset={props.result.offset}
            nullText={getSetting<string>("database.showNullAs", "<null>") ?? "<null>"}
            label={`${ds()?.name ?? props.dsId} · ${props.result.title}`}
            onShown={setRows}
            tools={setTools}
          />
        }
      >
        {(docs) => (
          <div class="db-json" data-testid="db-json">
            <For each={docs()}>{(d, i) => <pre class="db-json-doc" data-index={i() + 1 + props.result.offset}>{JSON.stringify(d, null, 2)}</pre>}</For>
          </div>
        )}
      </Show>
      <Show when={menu()}>{(m) => <ContextMenu {...m()} onClose={() => setMenu(null)} />}</Show>
    </div>
  );
}

export function ConsoleView(props: { tab: EditorTab }) {
  const loc = { id: consoleSourceId(props.tab.path) ?? "" };
  const opened = takeConsoleOptions(props.tab.path);
  const ds = () => sourceById(loc.id);
  const color = () => sourceColor(loc.id);
  const transactional = () => supportsTransactions(ds()?.driver ?? "mysql");
  const dialect = () => dialectOf(ds()?.driver ?? "mysql", ds()?.options.ddlDialect);
  const pageSize = () => ds()?.options.pageSize ?? getSetting<number>("database.pageSize", 500) ?? 500;

  let host!: HTMLDivElement;
  let body!: HTMLDivElement;
  let view: EditorView | undefined;
  const language = new Compartment();

  const [session, setSession] = createSignal<SessionInfo | null>(null);
  /** Use the data source's shared session instead of an own connection. */
  const [shared, setShared] = createSignal(!!ds()?.options.singleSession);
  const [running, setRunning] = createSignal(false);
  const [log, setLog] = createSignal<LogLine[]>([]);
  const [results, setResults] = createSignal<ResultTab[]>([]);
  const [active, setActive] = createSignal<"output" | number>("output");
  const [editorShare, setEditorShare] = createSignal(0.5);
  const [menu, setMenu] = createSignal<{ x: number; y: number; items: MenuItem[] } | null>(null);
  let resultSeq = 0;

  const autoCommit = () => !transactional() || (session()?.autoCommit ?? ds()?.options.autoCommit ?? true);
  const txOpen = () => !!session()?.txOpen;
  const schemaName = () => defaultSchema(loc.id);
  const prompt = () => schemaName() || ds()?.name || "db";

  const append = (kind: LogLine["kind"], text: string) => setLog((l) => [...l.slice(-400), { time: now(), kind, text }]);

  const unlisten = dbApi.onSession((s) => {
    if (s.id === session()?.id) setSession(s);
  });
  onCleanup(() => void unlisten.then((u) => u()));

  let opening: Promise<SessionInfo> | null = null;

  /** The console's session, opened once even when several actions ask at the same time. */
  function ensureSession(): Promise<SessionInfo> {
    const cur = session();
    if (cur) return Promise.resolve(cur);
    opening ??= openSession().finally(() => (opening = null));
    return opening;
  }

  async function openSession(): Promise<SessionInfo> {
    if (!(await connect(loc.id))) throw new Error(states[loc.id]?.error ?? `Cannot connect to ${ds()?.name ?? loc.id}`);
    const s = await dbApi.openSession(loc.id, shared());
    setSession(s);
    append("info", `Connected to ${ds()?.name ?? loc.id}${s.shared ? " (shared session)" : ""}`);
    if (opened.schema && dialect() === "mongo") {
      await dbApi.execute(s.id, `use ${opened.schema}`).catch(() => undefined);
      opened.schema = undefined;
    }
    return s;
  }

  async function run(sql: string, confirmed: boolean): Promise<StatementResult[]> {
    const s = await ensureSession();
    try {
      return await dbApi.execute(s.id, sql, pageSize(), confirmed);
    } catch (e) {
      // The connection was closed (Disconnect, data source changed): reconnect once.
      if (!/unknown console session/i.test(errorText(e))) throw e;
      setSession(null);
      const again = await ensureSession();
      return dbApi.execute(again.id, sql, pageSize(), confirmed);
    }
  }

  /** Read-only and production checks; false when the statement must not run. */
  async function allowed(sql: string): Promise<{ ok: boolean; confirmed: boolean }> {
    const g = await dbApi.guard(loc.id, sql).catch(() => ({}) as { blocked?: string; confirm?: string });
    if (g.blocked) {
      append("error", g.blocked);
      setActive("output");
      showBalloon(t("database.consoleView.readOnlyDataSource"), g.blocked);
      return { ok: false, confirmed: false };
    }
    if (g.confirm) {
      const ok = await askConfirm({ title: t("database.consoleView.productionDataSource"), message: g.confirm, confirm: t("database.consoleView.execute") });
      if (!ok) {
        append("info", "Execution cancelled");
        return { ok: false, confirmed: false };
      }
      return { ok: true, confirmed: true };
    }
    return { ok: true, confirmed: false };
  }

  async function execute(all = false): Promise<void> {
    if (!view || running()) return;
    const text = view.state.doc.toString();
    const sel = view.state.selection.main;
    const st = all
      ? { text, start: 0, end: text.length }
      : await dbApi.statementAt(text, sel.from, sel.to, dialect()).catch(() => null);
    if (!st || !st.text.trim() || !view) return;
    view.dispatch({ effects: flashEffect.of({ from: st.start, to: st.end }) });
    setTimeout(() => view?.dispatch({ effects: flashEffect.of(null) }), 900);
    // User parameters (?, :name, $1, ${name}, #{name}): their values first.
    const sqlDialect = consoleDialect(dialect());
    let sql: string | null = st.text;
    try {
      sql = sqlDialect ? await withParameters(props.tab.path, st.text, sqlDialect) : st.text;
    } catch (e) {
      append("error", errorText(e));
      setActive("output");
      return;
    }
    if (sql == null) {
      append("info", "Execution cancelled");
      return;
    }
    const check = await allowed(sql);
    if (!check.ok) return;
    setRunning(true);
    try {
      const res = await run(sql, check.confirmed);
      recordHistory(loc.id, props.tab.name, res);
      const tabs: ResultTab[] = [];
      let failed = false;
      for (const r of res) {
        append("sql", r.sql);
        if (r.error) {
          failed = true;
          append("error", r.error);
        } else if (r.returnsRows) {
          append("info", `${rowsText(r.rows.length)}${r.hasMore ? " (more available)" : ""} retrieved starting from 1 in ${r.elapsedMs} ms`);
          const table = sourceTable(r.sql);
          tabs.push({
            key: ++resultSeq,
            title: table ?? t("database.consoleView.resultLength1", { length1: tabs.length + 1 }),
            sql: r.sql,
            columns: r.columns,
            rows: r.rows,
            hasMore: r.hasMore,
            offset: 0,
            elapsedMs: r.elapsedMs,
            table,
            documents: r.documents,
          });
        } else {
          append("info", r.rowsAffected ? `${rowsText(r.rowsAffected)} affected in ${r.elapsedMs} ms` : `completed in ${r.elapsedMs} ms`);
        }
      }
      if (tabs.length) setResults(tabs);
      setActive(failed || !tabs.length ? "output" : 0);
    } catch (e) {
      append("error", errorText(e));
      setActive("output");
    } finally {
      setRunning(false);
    }
  }

  async function page(index: number, offset: number): Promise<void> {
    const r = results()[index];
    const s = session();
    if (!r || !s) return;
    try {
      const p = await dbApi.fetch(s.id, r.sql, offset, pageSize());
      setResults((list) =>
        list.map((x, i) =>
          i === index
            ? { ...x, rows: p.rows, hasMore: p.hasMore, offset, elapsedMs: p.elapsedMs, columns: p.columns.length ? p.columns : x.columns, documents: p.documents }
            : x,
        ),
      );
    } catch (e) {
      append("error", errorText(e));
    }
  }

  /** Explain Plan (Raw) of the selection or the statement at the caret: a Plan tab. */
  async function explain(raw: boolean): Promise<void> {
    if (!view || running()) return;
    const text = view.state.doc.toString();
    const sel = view.state.selection.main;
    const st = await dbApi.statementAt(text, sel.from, sel.to, dialect()).catch(() => null);
    if (!st || !st.text.trim()) return;
    view.dispatch({ effects: flashEffect.of({ from: st.start, to: st.end }) });
    setTimeout(() => view?.dispatch({ effects: flashEffect.of(null) }), 900);
    setRunning(true);
    try {
      const s = await ensureSession();
      let r: ExplainResult;
      try {
        r = await dbSchemaApi.explain(s.id, st.text, raw);
      } catch (e) {
        // An EXPLAIN ANALYZE on a production data source: ask like Execute does.
        const check = /production data source/i.test(errorText(e)) ? await allowed(st.text) : { ok: false, confirmed: false };
        if (!check.confirmed) throw e;
        r = await dbSchemaApi.explain(s.id, st.text, raw, true);
      }
      append("sql", r.sql);
      append("info", raw ? `${rowsText(r.rows.length)} of the plan retrieved in ${r.elapsedMs} ms` : r.plan ? t("database.consoleView.planBuiltInElapsedmsMs", { elapsedMs: r.elapsedMs }) : (r.error ?? t("database.consoleView.noPlan")));
      const tab: ResultTab = raw
        ? { key: ++resultSeq, title: t("tools.database.planRaw"), sql: r.sql, columns: r.columns, rows: r.rows, hasMore: false, offset: 0, elapsedMs: r.elapsedMs }
        : { key: ++resultSeq, title: t("database.consoleView.plan"), sql: r.sql, columns: r.columns, rows: r.rows, hasMore: false, offset: 0, elapsedMs: r.elapsedMs, explain: r };
      setResults((list) => [...list.filter((x) => x.title !== tab.title), tab]);
      setActive(results().length - 1);
    } catch (e) {
      append("error", errorText(e));
      setActive("output");
    } finally {
      setRunning(false);
    }
  }

  const cancel = () => {
    const s = session();
    if (s && running()) void dbApi.cancel(s.id);
  };

  async function setMode(auto: boolean): Promise<void> {
    try {
      const s = await ensureSession();
      setSession(await dbApi.setAutoCommit(s.id, auto));
      append("info", auto ? t("database.consoleView.switchedToAutoCommitMode") : t("database.consoleView.switchedToManualCommitMode"));
    } catch (e) {
      append("error", errorText(e));
    }
  }

  async function endTx(commit: boolean): Promise<void> {
    const s = session();
    if (!s) return;
    try {
      setSession(commit ? await dbApi.commit(s.id) : await dbApi.rollback(s.id));
      append("sql", commit ? "commit" : "rollback");
    } catch (e) {
      append("error", errorText(e));
    }
  }

  /** Switch between an own connection and the shared session (takes effect right away). */
  async function setSessionMode(share: boolean): Promise<void> {
    if (share === shared()) return;
    const old = session();
    setShared(share);
    setSession(null);
    if (old) await dbApi.closeSession(old.id).catch(() => undefined);
    try {
      await ensureSession();
    } catch (e) {
      append("error", errorText(e));
    }
  }

  /** Query History: insert a past statement at the end, or insert and run it. */
  const showHistory = () =>
    openHistory({
      dataSourceId: loc.id,
      console: props.tab.name,
      onInsert: (sql) => insertStatement(sql),
      onRun: (sql) => {
        insertStatement(sql);
        void execute();
      },
    });

  function insertStatement(sql: string): void {
    if (!view) return;
    const doc = view.state.doc;
    const sep = doc.length && !doc.toString().endsWith("\n") ? "\n" : "";
    const text = /;\s*$/.test(sql) ? sql : `${sql};`;
    const from = doc.length + sep.length;
    view.dispatch({ changes: { from: doc.length, insert: `${sep}${text}` }, selection: { anchor: from, head: from + text.length }, scrollIntoView: true });
    view.focus();
  }

  consoles.set(props.tab.path, { execute: () => execute(), cancel, focus: () => view?.focus(), history: showHistory, explain });
  onCleanup(() => {
    consoles.delete(props.tab.path);
    const s = session();
    if (s) void dbApi.closeSession(s.id).catch(() => undefined);
  });

  const schemaInfos = () => Object.values(states[loc.id]?.introspected ?? {});
  const languageExt = () => sqlWithSchema(dialect(), schemaInfos(), schemaName());

  onMount(() => {
    view = new EditorView({
      parent: host,
      state: EditorState.create({
        doc: props.tab.content,
        extensions: [
          lineNumbers(),
          highlightActiveLine(),
          highlightActiveLineGutter(),
          highlightSpecialChars(),
          drawSelection(),
          history(),
          indentOnInput(),
          bracketMatching(),
          closeBrackets(),
          autocompletion({ activateOnTyping: true }),
          highlightSelectionMatches(),
          findBar(),
          keymap.of([indentWithTab, ...defaultKeymap, ...historyKeymap, ...searchKeymap]),
          forgeEditorTheme,
          forgeHighlight,
          flashField,
          language.of(languageExt()),
          ...(consoleDialect(dialect()) ? sqlConsoleExtensions(props.tab.path) : []),
          EditorView.updateListener.of((u) => {
            if (u.docChanged) updateContent(props.tab.path, u.state.doc.toString());
          }),
        ],
      }),
    });
    const unregister = registerView(props.tab.path, view);
    onCleanup(() => {
      unregister();
      view?.destroy();
    });
    // Connect right away so completion knows the schema.
    void connect(loc.id).then((ok) => {
      if (ok && opened.run) void execute(true);
    });
    queueMicrotask(() => view?.focus());
  });

  // New metadata (connect, refresh): update completion.
  createEffect(on([schemaInfos, dialect, schemaName], () => view?.dispatch({ effects: language.reconfigure(languageExt()) }), { defer: true }));

  const startResize = (e: MouseEvent) => {
    e.preventDefault();
    const rect = body.getBoundingClientRect();
    const move = (ev: MouseEvent) => setEditorShare(Math.min(0.85, Math.max(0.15, (ev.clientY - rect.top) / rect.height)));
    const up = () => {
      window.removeEventListener("mousemove", move);
      window.removeEventListener("mouseup", up);
    };
    window.addEventListener("mousemove", move);
    window.addEventListener("mouseup", up);
  };

  const sessionMenu = (e: MouseEvent) => {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    setMenu({
      x: r.left,
      y: r.bottom + 2,
      items: [
        { label: t("database.consoleView.newSessionOwnConnection"), icon: !shared() ? <Check size={13} /> : undefined, run: () => void setSessionMode(false) },
        { label: t("database.consoleView.sharedSession"), icon: shared() ? <Check size={13} /> : undefined, run: () => void setSessionMode(true) },
      ],
    });
  };

  const txMenu = (e: MouseEvent) => {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    setMenu({
      x: r.left,
      y: r.bottom + 2,
      items: [
        { label: t("database.consoleView.auto"), icon: autoCommit() ? <Check size={13} /> : undefined, run: () => void setMode(true) },
        { label: t("database.consoleView.manual"), icon: !autoCommit() ? <Check size={13} /> : undefined, run: () => void setMode(false) },
      ],
    });
  };

  return (
    <div
      class="db-console"
      classList={{ colored: !!color(), production: color()?.name === "red" }}
      style={{ "--db-tint": color()?.tint, "--db-accent": color()?.accent }}
      data-db-path={props.tab.path}
    >
      <div class="db-toolbar">
        <button
          class="icon-btn db-tool db-run"
          title={t("tools.database.execute", { value: shortcutLabel("Console.Jdbc.Execute") })}
          disabled={running()}
          onClick={() => void execute()}
        >
          <Play size={15} />
        </button>
        <button class="icon-btn db-tool db-stop" title={t("database.consoleView.cancelRunningStatements")} disabled={!running()} onClick={cancel}>
          <Square size={13} />
        </button>
        <button class="icon-btn db-tool" title={t("tools.database.browseQueryHistory", { value: shortcutLabel("Console.History.Browse") })} data-testid="db-history" onClick={showHistory}>
          <History size={14} />
        </button>
        <Show when={consoleDialect(dialect())}>
          <button
            class="icon-btn db-tool"
            title={t("database.consoleView.explainPlan")}
            data-testid="db-explain"
            disabled={running()}
            onClick={(e) => {
              const r = e.currentTarget.getBoundingClientRect();
              setMenu({
                x: r.left,
                y: r.bottom + 2,
                items: [
                  { label: t("database.consoleView.explainPlan"), hint: shortcutLabel("Console.Jdbc.ExplainPlan"), icon: <ListTree size={13} />, run: () => void explain(false) },
                  { label: t("database.consoleView.explainPlanRaw"), hint: shortcutLabel("Console.Jdbc.ExplainPlan.Raw"), run: () => void explain(true) },
                ],
              });
            }}
          >
            <ListTree size={14} />
          </button>
        </Show>
        <span class="db-sep" />
        <button class="db-chip-btn" classList={{ open: shared() }} title={t("database.consoleView.session")} data-testid="db-session-mode" onClick={sessionMenu}>
          {shared() ? t("database.consoleView.sharedSession") : t("database.consoleView.newSession")}
          <ChevronDown size={12} />
        </button>
        <button
          class="db-chip-btn"
          classList={{ manual: !autoCommit(), open: txOpen() }}
          title={transactional() ? t("database.consoleView.transactionMode") : t("tools.database.consolesRunEveryStatementOn", { value: ds()?.driver ?? "" })}
          disabled={!transactional()}
          onClick={txMenu}
        >
          {t("tools.database.tx", { value: autoCommit() ? t("database.consoleView.auto") : t("database.consoleView.manual") })}
          <ChevronDown size={12} />
        </button>
        <button class="icon-btn db-tool db-commit" title={t("database.consoleView.commit")} disabled={autoCommit() || !txOpen()} onClick={() => void endTx(true)}>
          <Check size={15} />
        </button>
        <button class="icon-btn db-tool" title={t("database.consoleView.rollback")} disabled={autoCommit() || !txOpen()} onClick={() => void endTx(false)}>
          <Undo2 size={15} />
        </button>
        <Show when={running()}>
          <span class="db-running">
            <span class="db-spinner" /> {t("database.consoleView.executing")}
          </span>
        </Show>
        <span class="db-spacer" />
        <Show when={ds()?.options.readOnly}>
          <span class="db-badge" title={t("database.consoleView.readOnlyDataSourceChanges")}>
            <Lock size={11} /> read-only
          </span>
        </Show>
        <Show when={color()}>
          {(c) => (
            <span class="db-badge colored" data-testid="db-color-badge" title={DS_COLORS[c().name]?.label ?? c().name}>
              {c().name === "red" ? "production" : c().name === "yellow" ? "staging" : c().name === "green" ? "development" : c().name}
            </span>
          )}
        </Show>
        <button class="db-target" title={t("database.consoleView.dataSourceProperties")} onClick={() => openDataSources({ select: loc.id })}>
          <Database size={13} />
          <span>{ds()?.name ?? loc.id}</span>
          <Show when={schemaName()}>
            <ChevronRight size={12} class="muted" />
            <span class="muted">{schemaName()}</span>
          </Show>
        </button>
      </div>
      <div class="db-console-body" ref={body}>
        <div class="db-console-editor" style={{ flex: `0 0 ${results().length || log().length ? `${editorShare() * 100}%` : "100%"}` }}>
          <div class="code-editor" ref={host} />
        </div>
        <Show when={results().length || log().length}>
          <div class="db-hsplit" onMouseDown={startResize} />
          <div class="db-results">
            <div class="db-result-tabs" role="tablist">
              <button class="db-result-tab" classList={{ active: active() === "output" }} onClick={() => setActive("output")}>
                <ScrollText size={13} />
                {t("database.consoleView.output")}
              </button>
              <For each={results()}>
                {(r, i) => (
                  <button class="db-result-tab" classList={{ active: active() === i() }} onClick={() => setActive(i())} title={r.sql}>
                    <Show when={r.explain || r.title === t("tools.database.planRaw")} fallback={<Table2 size={13} />}>
                      <ListTree size={13} />
                    </Show>
                    {r.title}
                  </button>
                )}
              </For>
            </div>
            <Show
              when={active() !== "output" && results()[active() as number]}
              fallback={
                <div class="db-output" tabIndex={0}>
                  <For each={log()}>
                    {(l) => (
                      <div class={`db-log ${l.kind}`}>
                        <Show when={l.kind === "sql"} fallback={<span class="db-log-time">[{l.time}]</span>}>
                          <span class="db-log-prompt">{prompt()}&gt;</span>
                        </Show>
                        <span class="db-log-text">{l.text}</span>
                      </div>
                    )}
                  </For>
                </div>
              }
            >
              {(r) => (
                <Show when={r().explain} fallback={<ResultView result={r()} dsId={loc.id} pageSize={pageSize()} onPage={(o) => void page(active() as number, o)} />}>
                  {(x) => <PlanView result={x()} />}
                </Show>
              )}
            </Show>
          </div>
        </Show>
      </div>
      <Show when={menu()}>{(m) => <ContextMenu {...m()} onClose={() => setMenu(null)} />}</Show>
    </div>
  );
}
