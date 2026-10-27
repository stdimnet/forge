/**
 * The SQL editor's dialogs: Parameters (values of user parameters before running; the last
 * values are remembered per console), Query History (search, insert or re-run past
 * statements of a data source) and Run SQL Script (data source, stop on error, then the
 * statements' progress and results).
 */
import { createMemo, createResource, createSignal, For, onCleanup, onMount, Show, type JSX } from "solid-js";
import { CircleCheck, CircleX, LoaderCircle, Search } from "lucide-solid";
import { suspendKeymap, showBalloon } from "@/keymap";
import { askConfirm } from "@/features/contextMenu/dialogs";
import { getSetting, saveSetting } from "@/state/settings";
import { DRIVER_LABELS } from "@/ipc/db";
import { sqlApi, type HistoryEntry, type RunFileEvent, type RunFileSummary } from "@/ipc/sql";
import { sources } from "@/features/database/store";

const message = (e: unknown) => String(e instanceof Error ? e.message : e);

type Dialog =
  | { kind: "params"; names: string[]; values: Record<string, string>; sql: string; done: (v: Record<string, string> | null) => void }
  | { kind: "history"; dataSourceId: string; console?: string; onInsert: (sql: string) => void; onRun: (sql: string) => void }
  | { kind: "runFile"; path: string; dataSourceId?: string };

const [dialog, setDialog] = createSignal<Dialog | null>(null);
const close = () => setDialog(null);

/** Ask the values of `names` (starting with `values`); `null` when cancelled. */
export function askParams(names: string[], values: Record<string, string>, sql: string): Promise<Record<string, string> | null> {
  return new Promise((done) => setDialog({ kind: "params", names, values, sql, done }));
}

export function openHistory(opts: Omit<Extract<Dialog, { kind: "history" }>, "kind">): void {
  setDialog({ kind: "history", ...opts });
}

export function openRunFile(path: string, dataSourceId?: string): void {
  setDialog({ kind: "runFile", path, dataSourceId });
}

export function SqlDialogsHost() {
  return (
    <Show when={dialog()}>
      {(d) => {
        const v = d();
        if (v.kind === "params") return <ParamsDialog {...v} />;
        if (v.kind === "history") return <HistoryDialog {...v} />;
        return <RunFileDialog {...v} />;
      }}
    </Show>
  );
}

function Modal(props: {
  title: string;
  testId: string;
  class?: string;
  onClose: () => void;
  onEnter?: () => void;
  footer: JSX.Element;
  children: JSX.Element;
}) {
  let root!: HTMLDivElement;
  const release = suspendKeymap();
  onCleanup(release);
  onMount(() => root.querySelector<HTMLElement>("input, select, [tabindex]")?.focus());
  return (
    <div class="db-modal-backdrop" onMouseDown={(e) => e.target === e.currentTarget && props.onClose()}>
      <div
        ref={root}
        class={`db-modal sql-modal ${props.class ?? ""}`}
        role="dialog"
        aria-label={props.title}
        data-testid={props.testId}
        onKeyDown={(e) => {
          if (e.key === "Escape") props.onClose();
          else if (e.key === "Enter" && props.onEnter && (e.target as HTMLElement).tagName !== "BUTTON") props.onEnter();
          else return;
          e.preventDefault();
          e.stopPropagation();
        }}
      >
        <header class="db-modal-title">{props.title}</header>
        <div class="db-modal-body">{props.children}</div>
        <footer class="db-modal-actions">{props.footer}</footer>
      </div>
    </div>
  );
}

// ---- Parameters ----------------------------------------------------------------------------

function ParamsDialog(props: Extract<Dialog, { kind: "params" }>) {
  const [values, setValues] = createSignal<Record<string, string>>({ ...props.values });
  const finish = (v: Record<string, string> | null) => {
    close();
    props.done(v);
  };
  const ok = () => finish(values());
  return (
    <Modal
      title="Parameters"
      testId="sql-params-dialog"
      onClose={() => finish(null)}
      onEnter={ok}
      footer={
        <>
          <span class="db-doc-spacer" />
          <button class="btn-secondary" onClick={() => finish(null)}>
            Cancel
          </button>
          <button class="btn-primary" onClick={ok}>
            Execute
          </button>
        </>
      }
    >
      <div class="sql-params-hint">Values are inserted as written: quote strings ('text').</div>
      <table class="set-table sql-params-table">
        <thead>
          <tr>
            <th>Name</th>
            <th>Value</th>
          </tr>
        </thead>
        <tbody>
          <For each={props.names}>
            {(name) => (
              <tr>
                <td class="sql-param-name">{name}</td>
                <td>
                  <input
                    class="set-input sql-param-value"
                    value={values()[name] ?? ""}
                    spellcheck={false}
                    data-param={name}
                    onInput={(e) => {
                      const v = e.currentTarget.value;
                      setValues((cur) => ({ ...cur, [name]: v }));
                    }}
                  />
                </td>
              </tr>
            )}
          </For>
        </tbody>
      </table>
      <pre class="sql-params-preview">{props.sql}</pre>
    </Modal>
  );
}

// ---- Query History -------------------------------------------------------------------------

const pad = (n: number) => String(n).padStart(2, "0");
export const timeText = (ms: number) => {
  const d = new Date(ms);
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
};

function HistoryDialog(props: Extract<Dialog, { kind: "history" }>) {
  const [text, setText] = createSignal("");
  const [onlyConsole, setOnlyConsole] = createSignal(false);
  const [sel, setSel] = createSignal(0);
  const [version, setVersion] = createSignal(0);
  const [entries] = createResource(
    () => ({ text: text(), console: onlyConsole() ? props.console : undefined, v: version() }),
    (q) => sqlApi.history(props.dataSourceId, { text: q.text, console: q.console }).catch(() => [] as HistoryEntry[]),
  );
  const current = () => entries()?.[sel()];
  const pick = (run: boolean) => {
    const e = current();
    if (!e) return;
    close();
    if (run) props.onRun(e.sql);
    else props.onInsert(e.sql);
  };
  const clear = async () => {
    const ok = await askConfirm({ title: "Clear Query History", message: `Forget every statement run on ${sources().find((s) => s.id === props.dataSourceId)?.name ?? props.dataSourceId}?`, confirm: "Clear" });
    if (!ok) return;
    await sqlApi.clearHistory(props.dataSourceId).catch((e) => showBalloon("Cannot clear the history", message(e)));
    setVersion((v) => v + 1);
  };
  const name = () => sources().find((s) => s.id === props.dataSourceId)?.name ?? props.dataSourceId;
  return (
    <Modal
      title={`Query History: ${name()}`}
      testId="sql-history-dialog"
      class="sql-history"
      onClose={close}
      onEnter={() => pick(false)}
      footer={
        <>
          <button class="btn-secondary" onClick={() => void clear()}>
            Clear History
          </button>
          <span class="db-doc-spacer" />
          <button class="btn-secondary" disabled={!current()} onClick={() => pick(true)}>
            Run
          </button>
          <button class="btn-primary" disabled={!current()} onClick={() => pick(false)}>
            Insert
          </button>
        </>
      }
    >
      <div class="sql-history-bar">
        <label class="sql-history-search">
          <Search size={13} class="muted" />
          <input
            value={text()}
            placeholder="Search statements"
            spellcheck={false}
            onInput={(e) => {
              setText(e.currentTarget.value);
              setSel(0);
            }}
            onKeyDown={(e) => {
              const n = entries()?.length ?? 0;
              if (e.key === "ArrowDown") setSel((s) => Math.min(n - 1, s + 1));
              else if (e.key === "ArrowUp") setSel((s) => Math.max(0, s - 1));
              else return;
              e.preventDefault();
            }}
          />
        </label>
        <Show when={props.console}>
          <label class="set-check">
            <input type="checkbox" checked={onlyConsole()} onChange={(e) => setOnlyConsole(e.currentTarget.checked)} />
            <span>This console only</span>
          </label>
        </Show>
      </div>
      <div class="sql-history-list" tabIndex={0}>
        <For each={entries() ?? []} fallback={<div class="set-list-empty">{text() ? "No statements match" : "No statements ran on this data source yet"}</div>}>
          {(e, i) => (
            <div class="sql-history-row" classList={{ selected: sel() === i(), failed: !!e.error }} onClick={() => setSel(i())} onDblClick={() => pick(false)}>
              <span class="sql-history-time">{timeText(e.time)}</span>
              <span class="sql-history-sql">{e.sql.replace(/\s+/g, " ")}</span>
              <span class="sql-history-meta">
                {e.error ? "failed" : e.rows != null ? `${e.rows} row${e.rows === 1 ? "" : "s"}` : ""} · {e.durationMs} ms
              </span>
            </div>
          )}
        </For>
      </div>
      <Show when={current()}>
        {(e) => (
          <div class="sql-history-preview">
            <div class="sql-history-preview-head muted">
              {e().console || "console"} · {timeText(e().time)} · {e().durationMs} ms
              <Show when={e().error}>{(err) => <span class="sql-error"> · {err()}</span>}</Show>
            </div>
            <pre>{e().sql}</pre>
          </div>
        )}
      </Show>
    </Modal>
  );
}

// ---- Run SQL Script ------------------------------------------------------------------------

function RunFileDialog(props: Extract<Dialog, { kind: "runFile" }>) {
  const candidates = createMemo(() => sources().filter((s) => !["ddl", "redis", "mongodb"].includes(s.driver)));
  const [ds, setDs] = createSignal(props.dataSourceId && candidates().some((s) => s.id === props.dataSourceId) ? props.dataSourceId : (candidates()[0]?.id ?? ""));
  const [stopOnError, setStopOnError] = createSignal(getSetting<boolean>("database.runFile.stopOnError", true) ?? true);
  const [phase, setPhase] = createSignal<"setup" | "running" | "done">("setup");
  const [events, setEvents] = createSignal<RunFileEvent[]>([]);
  const [total, setTotal] = createSignal(0);
  const [summary, setSummary] = createSignal<RunFileSummary | null>(null);
  const [error, setError] = createSignal<string | null>(null);
  const runId = `run-${Date.now()}-${Math.random().toString(36).slice(2, 7)}`;
  const unlisten = sqlApi.onRunFile((e) => {
    if (e.runId !== runId) return;
    setTotal(e.total);
    if (e.kind === "statement") setEvents((list) => [...list, e]);
  });
  onCleanup(() => void unlisten.then((u) => u()));
  const fileName = props.path.split("/").pop() ?? props.path;

  const run = async (confirmed = false) => {
    if (!ds()) return;
    void saveSetting("database.runFile.stopOnError", stopOnError());
    setPhase("running");
    setEvents([]);
    setError(null);
    try {
      const res = await sqlApi.runFile({ runId, path: props.path, dataSourceId: ds(), stopOnError: stopOnError(), confirmed });
      if (res.needsConfirmation) {
        const ok = await askConfirm({ title: "Production Data Source", message: res.needsConfirmation, confirm: "Execute" });
        if (ok) return run(true);
        setPhase("setup");
        return;
      }
      setSummary(res);
      setPhase("done");
      showBalloon(
        `${fileName}: ${res.executed} of ${res.total} statements executed`,
        res.failed ? `${res.failed} failed` : res.cancelled ? "Stopped" : `Finished in ${res.elapsedMs} ms`,
      );
    } catch (e) {
      setError(message(e));
      setPhase("done");
    }
  };
  const stop = () => void sqlApi.cancelRunFile(runId);
  const done = () => events().length;
  const percent = () => (total() ? Math.round((done() / total()) * 100) : 0);
  return (
    <Modal
      title={`Run SQL Script: ${fileName}`}
      testId="sql-run-file-dialog"
      class="sql-run-file"
      onClose={() => phase() !== "running" && close()}
      onEnter={() => phase() === "setup" && void run()}
      footer={
        <>
          <span class="db-doc-spacer" />
          <Show when={phase() === "setup"}>
            <button class="btn-secondary" onClick={close}>
              Cancel
            </button>
            <button class="btn-primary" disabled={!ds()} onClick={() => void run()}>
              Run
            </button>
          </Show>
          <Show when={phase() === "running"}>
            <button class="btn-secondary" onClick={stop}>
              Stop
            </button>
          </Show>
          <Show when={phase() === "done"}>
            <button class="btn-primary" onClick={close}>
              Close
            </button>
          </Show>
        </>
      }
    >
      <Show
        when={phase() !== "setup"}
        fallback={
          <div class="set-section-body">
            <div class="set-row">
              <div class="set-line">
                <span class="set-label">Data source:</span>
                <select class="set-input set-select" value={ds()} onChange={(e) => setDs(e.currentTarget.value)} data-testid="sql-run-ds">
                  <For each={candidates()} fallback={<option value="">No data sources</option>}>
                    {(s) => (
                      <option value={s.id}>
                        {s.name} ({DRIVER_LABELS[s.driver]})
                      </option>
                    )}
                  </For>
                </select>
              </div>
            </div>
            <div class="set-row">
              <label class="set-check">
                <input type="checkbox" checked={stopOnError()} onChange={(e) => setStopOnError(e.currentTarget.checked)} />
                <span>Stop at the first error</span>
              </label>
            </div>
            <div class="set-row">
              <div class="set-desc">Statements run in order on a new session of the data source; read-only and production data sources are guarded like consoles.</div>
            </div>
          </div>
        }
      >
        <div class="sql-run-progress">
          <div class="sql-run-bar">
            <div class="sql-run-fill" classList={{ failed: !!summary()?.failed || !!error() }} style={{ width: `${phase() === "done" && !error() ? 100 : percent()}%` }} />
          </div>
          <div class="sql-run-status">
            <Show when={phase() === "running"}>
              <LoaderCircle size={13} class="db-spin" />
            </Show>
            <span>
              {phase() === "running"
                ? `Executing ${Math.min(done() + 1, Math.max(total(), 1))} of ${total() || "…"}`
                : error()
                  ? error()
                  : `${summary()?.executed ?? 0} of ${summary()?.total ?? 0} statements executed${summary()?.failed ? `, ${summary()!.failed} failed` : ""}${summary()?.cancelled ? ", stopped" : ""} in ${summary()?.elapsedMs ?? 0} ms`}
            </span>
          </div>
        </div>
        <div class="sql-run-list" data-testid="sql-run-results">
          <For each={events()}>
            {(e) => (
              <div class="sql-run-row" classList={{ failed: !e.ok }}>
                {e.ok ? <CircleCheck size={13} class="sql-ok" /> : <CircleX size={13} class="sql-error" />}
                <span class="sql-run-line muted">{e.line + 1}</span>
                <span class="sql-run-sql">{(e.sql ?? "").replace(/\s+/g, " ")}</span>
                <span class="sql-run-meta">{e.ok ? `${e.rows != null ? `${e.rows} row${e.rows === 1 ? "" : "s"} · ` : ""}${e.elapsedMs} ms` : e.error}</span>
              </div>
            )}
          </For>
        </div>
      </Show>
    </Modal>
  );
}
