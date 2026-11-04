/**
 * Sessions view of the Database tool window (DataGrip's sessions in Services): every open
 * console session with its data source, transaction mode, open transaction (statements and
 * age), the running statement, and Commit / Rollback / Cancel / Close.
 */
import { createSignal, For, onCleanup, onMount, Show } from "solid-js";
import { Check, Database, Square, Undo2, Unplug, Users } from "lucide-solid";
import { dbApi, type SessionInfo } from "@/ipc/db";
import { showBalloon } from "@/keymap";
import { loadSessions, sessionList, sourceById, sourceColor } from "./store";
import { t as tr } from "@/i18n";

const errorText = (e: unknown) => (e instanceof Error ? e.message : String(e));

/** `3 min`, `42 s` since a Unix millisecond time. */
export function since(ms: number | undefined, now: number): string {
  if (!ms) return "";
  const s = Math.max(0, Math.round((now - ms) / 1000));
  if (s < 60) return `${s} s`;
  if (s < 3600) return `${Math.floor(s / 60)} min`;
  return `${Math.floor(s / 3600)} h ${Math.floor((s % 3600) / 60)} min`;
}

function SessionRow(props: { s: SessionInfo; now: number }) {
  const ds = () => sourceById(props.s.dataSourceId);
  const color = () => sourceColor(props.s.dataSourceId);
  const act = (what: string, f: () => Promise<unknown>) => () =>
    void f()
      .then(() => loadSessions())
      .catch((e) => showBalloon(`${what} failed`, errorText(e)));
  return (
    <div class="db-session" classList={{ running: props.s.running, tx: props.s.txOpen }} data-testid="db-session" style={{ "--db-accent": color()?.accent }}>
      <div class="db-session-head">
        <span class="db-session-state" title={props.s.running ? tr("database.sessionsView.running") : tr("database.sessionsView.idle")} />
        <Database size={13} class="db-session-ds" />
        <span class="db-session-name">{ds()?.name ?? props.s.dataSourceId}</span>
        <span class="db-session-kind">{props.s.shared ? "shared session" : "console session"}</span>
        <Show when={(props.s.consoles ?? 1) > 1}>
          <span class="db-session-badge" title={tr("database.sessionsView.consolesUsingThisSession")}>
            <Users size={11} /> {props.s.consoles}
          </span>
        </Show>
        <span class="db-spacer" />
        <button class="icon-btn db-tool" title={tr("database.sessionsView.commit")} disabled={!props.s.txOpen} onClick={act("Commit", () => dbApi.commit(props.s.id))}>
          <Check size={14} />
        </button>
        <button class="icon-btn db-tool" title={tr("database.sessionsView.rollback")} disabled={!props.s.txOpen} onClick={act("Rollback", () => dbApi.rollback(props.s.id))}>
          <Undo2 size={14} />
        </button>
        <button class="icon-btn db-tool" title={tr("database.sessionsView.cancelRunningStatement")} disabled={!props.s.running} onClick={act("Cancel", () => dbApi.cancel(props.s.id))}>
          <Square size={12} />
        </button>
        <button class="icon-btn db-tool" title={tr("database.sessionsView.closeSession")} onClick={act("Close", () => dbApi.terminateSession(props.s.id))}>
          <Unplug size={13} />
        </button>
      </div>
      <div class="db-session-meta">
        <span>{props.s.transactions === false ? tr("database.sessionsView.txNone") : props.s.autoCommit ? tr("database.sessionsView.txAuto") : tr("database.sessionsView.txManual")}</span>
        <Show
          when={props.s.txOpen}
          fallback={<span class="muted">{props.s.running ? "running" : "idle"} · open {since(props.s.openedAt, props.now)}</span>}
        >
          <span class="db-session-tx">
            transaction open · {props.s.txStatements ?? 0} statement{props.s.txStatements === 1 ? "" : "s"} · {since(props.s.txStartedAt, props.now)}
          </span>
        </Show>
      </div>
      <Show when={props.s.lastSql}>
        <div class="db-session-sql" title={props.s.lastSql}>
          {props.s.lastSql}
        </div>
      </Show>
    </div>
  );
}

export function SessionsView() {
  const [now, setNow] = createSignal(Date.now());
  onMount(() => {
    void loadSessions();
    const t = setInterval(() => setNow(Date.now()), 1000);
    onCleanup(() => clearInterval(t));
  });
  return (
    <div class="db-sessions" data-testid="db-sessions">
      <div class="db-sessions-title">{tr("database.sessionsView.sessions")}</div>
      <For
        each={sessionList()}
        fallback={
          <div class="db-empty">
            <Unplug size={26} class="db-empty-icon" />
            <div>{tr("database.sessionsView.noOpenSessions")}</div>
            <div class="muted">{tr("database.sessionsView.consolesOpenASessionWhen")}</div>
          </div>
        }
      >
        {(s) => <SessionRow s={s} now={now()} />}
      </For>
    </div>
  );
}
