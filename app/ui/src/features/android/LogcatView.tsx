/**
 * The Logcat tool window, like Android Studio's: the log of a device, parsed and filtered in
 * Rust (`adb logcat -v threadtime`, Android Studio's query language: `package:mine
 * level:warn tag:OkHttp -tag:Choreographer is:crash text`), with colors per level and
 * stack trace frames that open the project's sources. The toolbar picks the device,
 * clears the log, pauses the view, scrolls to the end and restarts the stream.
 */
import { createEffect, createMemo, createSignal, For, on, onMount, Show } from "solid-js";
import { ArrowDownToLine, CircleAlert, Filter, Pause, Play, RotateCcw, Trash2 } from "lucide-solid";
import type { LogEntry, LogLevel } from "@/ipc/android";
import { revealLocation } from "@/features/lsp/navigation";
import { showBalloon } from "@/keymap";
import {
  clearLogcat,
  deviceLabel,
  devicesSnapshot,
  logcatCounts,
  logcatEntries,
  logcatError,
  logcatPaused,
  logcatQuery,
  logcatSerial,
  logcatStreaming,
  onlineDevices,
  refreshDevices,
  setQuery,
  startLogcat,
  togglePause,
} from "./store";
import { t } from "@/i18n";
import "./android.css";

const LEVELS: { level: LogLevel; label: string }[] = [
  { level: "V", label: t("langs.android.verbose") },
  { level: "D", label: t("common.debug2") },
  { level: "I", label: t("langs.android.info") },
  { level: "W", label: t("langs.android.warn") },
  { level: "E", label: t("langs.android.error") },
  { level: "A", label: t("langs.android.assert") },
];

/** Character index of a UTF-8 byte offset of `s`. */
function charIndex(s: string, byte: number): number {
  let bytes = 0;
  let i = 0;
  for (const ch of s) {
    if (bytes >= byte) break;
    const c = ch.codePointAt(0)!;
    bytes += c < 0x80 ? 1 : c < 0x800 ? 2 : c < 0x10000 ? 3 : 4;
    i += ch.length;
  }
  return i;
}

async function openFrame(e: LogEntry) {
  if (!e.link) return;
  const ok = await revealLocation(e.link.path, { line: Math.max(0, e.link.line - 1), character: 0 });
  if (!ok) showBalloon(t("langs.android.cannotOpen", { path: e.link.path }));
}

function Message(props: { entry: LogEntry }) {
  const parts = () => {
    const e = props.entry;
    if (!e.link) return null;
    const a = charIndex(e.message, e.link.start);
    const b = charIndex(e.message, e.link.end);
    return [e.message.slice(0, a), e.message.slice(a, b), e.message.slice(b)] as const;
  };
  return (
    <Show when={parts()} fallback={<span class="logcat-msg">{props.entry.message}</span>}>
      {(p) => (
        <span class="logcat-msg">
          {p()[0]}
          <a class="logcat-link" title={`${props.entry.link!.path}:${props.entry.link!.line}`} onClick={() => void openFrame(props.entry)}>
            {p()[1]}
          </a>
          {p()[2]}
        </span>
      )}
    </Show>
  );
}

/** The level term of a query (`level:warn`) and the query without it. */
function splitLevel(q: string): { level: LogLevel | ""; rest: string } {
  const m = /(^|\s)level:(\w+)/i.exec(q);
  if (!m) return { level: "", rest: q };
  const v = m[2]!.toUpperCase();
  const level = (LEVELS.find((l) => l.level === v[0] || l.label.toUpperCase() === v)?.level ?? "") as LogLevel | "";
  return { level, rest: (q.slice(0, m.index) + q.slice(m.index + m[0].length)).replace(/\s+/g, " ").trim() };
}

export function LogcatView() {
  let scroller: HTMLDivElement | undefined;
  const [follow, setFollow] = createSignal(true);
  onMount(async () => {
    if (!devicesSnapshot()) await refreshDevices();
    if (!logcatSerial()) {
      const first = onlineDevices()[0];
      if (first) void startLogcat(first.serial);
    }
  });
  const level = createMemo(() => splitLevel(logcatQuery()).level);
  const setLevel = (l: LogLevel | "") => {
    const rest = splitLevel(logcatQuery()).rest;
    const label = LEVELS.find((x) => x.level === l)?.label.toLowerCase();
    setQuery(l ? `${rest} level:${label}`.trim() : rest);
  };
  createEffect(
    on(logcatEntries, () => {
      if (follow() && scroller) queueMicrotask(() => scroller && (scroller.scrollTop = scroller.scrollHeight));
    }),
  );
  return (
    <div class="logcat" data-testid="logcat">
      <div class="logcat-toolbar">
        <select
          class="logcat-device"
          title={t("langs.android.device")}
          value={logcatSerial() ?? ""}
          onChange={(e) => e.currentTarget.value && void startLogcat(e.currentTarget.value)}
          data-testid="logcat-device"
        >
          <Show when={!onlineDevices().length}>
            <option value="">{t("langs.android.noConnectedDevices")}</option>
          </Show>
          <For each={onlineDevices()}>{(d) => <option value={d.serial}>{`${deviceLabel(d.serial)} (${d.serial})`}</option>}</For>
        </select>
        <div class="logcat-filter">
          <Filter size={13} class="muted" />
          <input
            type="text"
            spellcheck={false}
            placeholder={t("langs.android.packageMineLevelWarnTag")}
            value={logcatQuery()}
            onInput={(e) => setQuery(e.currentTarget.value)}
            data-testid="logcat-query"
          />
        </div>
        <select class="logcat-level" title={t("langs.android.minimumLevel")} value={level()} onChange={(e) => setLevel(e.currentTarget.value as LogLevel | "")} data-testid="logcat-level">
          <option value="">{t("langs.android.allLevels")}</option>
          <For each={LEVELS}>{(l) => <option value={l.level}>{l.label}</option>}</For>
        </select>
        <span class="java-toolbar-sep" />
        <button class="icon-btn" title={t("langs.android.clearLogcat")} onClick={() => void clearLogcat()}>
          <Trash2 size={14} />
        </button>
        <button class="icon-btn" classList={{ active: logcatPaused() }} title={logcatPaused() ? t("langs.android.resumeLogcat") : t("langs.android.pauseLogcat")} onClick={togglePause}>
          {logcatPaused() ? <Play size={14} /> : <Pause size={14} />}
        </button>
        <button class="icon-btn" classList={{ active: follow() }} title={t("common.scrollToTheEnd")} onClick={() => setFollow(!follow())}>
          <ArrowDownToLine size={14} />
        </button>
        <button class="icon-btn" title={t("langs.android.restartLogcat")} disabled={!logcatSerial()} onClick={() => logcatSerial() && void startLogcat(logcatSerial()!)}>
          <RotateCcw size={14} />
        </button>
        <span class="muted logcat-counts">
          {logcatCounts().matched} of {logcatCounts().total}
          {logcatStreaming() ? "" : t("langs.android.stopped")}
        </span>
      </div>
      <Show when={logcatError()}>
        <div class="android-hint">
          <CircleAlert size={14} class="java-error-icon" />
          <span>{logcatError()}</span>
        </div>
      </Show>
      <div
        class="logcat-rows"
        ref={scroller}
        onScroll={(e) => {
          const el = e.currentTarget;
          const atEnd = el.scrollHeight - el.scrollTop - el.clientHeight < 24;
          if (atEnd !== follow()) setFollow(atEnd);
        }}
      >
        <For each={logcatEntries()} fallback={<div class="muted android-empty">{logcatSerial() ? t("langs.android.noEntriesMatchTheFilter") : t("langs.android.selectARunningDevice")}</div>}>
          {(e) => (
            <div class={`logcat-row level-${e.level}`} classList={{ stack: e.stack }} data-level={e.level}>
              <span class="logcat-time">{`${e.date} ${e.time}`}</span>
              <span class="logcat-pid">{`${e.pid}-${e.tid}`}</span>
              <span class="logcat-tag" title={e.tag}>{e.tag}</span>
              <span class="logcat-pkg" title={e.package ?? ""}>{e.package ?? `pid-${e.pid}`}</span>
              <span class={`logcat-level-badge level-${e.level}`}>{e.level}</span>
              <Message entry={e} />
            </div>
          )}
        </For>
      </div>
    </div>
  );
}
