/**
 * Laravel tool window: an Overview tab (application, environment, counts, models, artisan
 * commands, ide.json completions and code generations) and a Logs tab with the entries of
 * `storage/logs/*.log` (level filter, search, follow, stack traces with clickable
 * file:line links). Parsing happens in Rust (`laravel_overview`, `laravel_logs`).
 */
import { createEffect, createMemo, createSignal, For, Match, on, onCleanup, Show, Switch } from "solid-js";
import { ArrowDownToLine, ChevronDown, ChevronRight, Database, Play, RefreshCw, Search, SquareTerminal, Wand2, X } from "lucide-solid";
import { laravelApi, type LaravelLogEntry, type LaravelLogPage, type LaravelOverview, type LogLevel, type LogStackFrame } from "@/ipc/laravel";
import { openRunCommand } from "@/features/cliTools/CommandPopup";
import { tools as cliTools } from "@/features/cliTools/store";
import { askName } from "@/features/contextMenu/dialogs";
import { revealLocation } from "@/features/lsp/navigation";
import { showBalloon } from "@/keymap";
import { laravelActive, laravelSetting, reveal } from "./state";
import { t as tr } from "@/i18n";
import { latestOnly } from "@/state/async";
import { onProject } from "@/state/projectLifecycle";

const [tab, setTab] = createSignal<"overview" | "logs">("overview");
export { setTab as setLaravelTab };
/** The log page shown (exported for tests). */
export const laravelLogPage = () => page();

const [overview, setOverview] = createSignal<LaravelOverview | null>(null);
const [overviewLoading, setOverviewLoading] = createSignal(false);
const [page, setPage] = createSignal<LaravelLogPage | null>(null);
const [logFile, setLogFile] = createSignal("");
const [logsLoading, setLogsLoading] = createSignal(false);

export async function loadOverview(): Promise<void> {
  setOverviewLoading(true);
  try {
    setOverview(await laravelApi.overview());
  } catch (e) {
    console.warn("Laravel overview failed", e);
  } finally {
    setOverviewLoading(false);
  }
}

// Only the newest read applies: an older one (the previously selected file, or the previous
// project) arriving last would show the wrong log under the new selection.
const logReads = latestOnly();

export async function loadLogs(): Promise<void> {
  const current = logReads.begin();
  setLogsLoading(true);
  try {
    const p = await laravelApi.logs(logFile() || undefined);
    if (!current()) return;
    setPage(p);
    if (!logFile() && p.path) setLogFile(p.path);
  } catch (e) {
    if (current()) console.warn("Laravel logs failed", e);
  } finally {
    if (current()) setLogsLoading(false);
  }
}

// The selected log file and the entries belong to the project.
onProject(
  "closing",
  () => {
    logReads.invalidate();
    setLogsLoading(false);
    setPage(null);
    setLogFile("");
    setOverview(null);
  },
  { name: "laravel logs" },
);

const LEVELS: LogLevel[] = ["debug", "info", "notice", "warning", "error", "critical", "alert", "emergency"];
const rank = (l: LogLevel) => LEVELS.indexOf(l);

function size(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

const artisanAlias = () => cliTools().find((t) => t.kind === "artisan" && t.enabled)?.alias ?? "a";

// ---------------------------------------------------------------- Overview

function Fact(props: { label: string; value?: string | number | null; mono?: boolean }) {
  return (
    <div class="lv-fact">
      <span class="lv-fact-label">{props.label}</span>
      <span class="lv-fact-value" classList={{ mono: props.mono }}>
        {props.value ?? "—"}
      </span>
    </div>
  );
}

async function runGeneration(id: string, name: string, suffix?: string) {
  const input = await askName({
    title: name,
    label: suffix ? tr("langs.laravel.classNameTheSuffixIs", { suffix }) : tr("langs.laravel.className"),
    initial: "",
    confirm: tr("common.create"),
    validate: (v) => (/^[A-Za-z_][\w\\/]*$/.test(v.trim()) ? null : "Enter a class name, e.g. Orders/Invoice"),
  });
  if (!input) return;
  try {
    const files = await laravelApi.generate(id, input, true);
    const created = files.filter((f) => !f.exists);
    const skipped = files.filter((f) => f.exists);
    if (created[0]) void revealLocation(created[0].path, { line: 0, character: 0 });
    showBalloon(
      created.length ? `${name}: created ${created.map((f) => f.path).join(", ")}` : tr("langs.laravel.nothingCreated", { name }),
      skipped.length ? `Already exists: ${skipped.map((f) => f.path).join(", ")}` : undefined,
      4000,
      undefined,
      { group: "Laravel" },
    );
    void loadOverview();
  } catch (e) {
    showBalloon(`${name} failed`, String(e), 5000, undefined, { group: "Laravel", type: "error" });
  }
}

function OverviewTab() {
  createEffect(on(laravelActive, (a) => a && !overview() && !overviewLoading() && void loadOverview()));
  return (
    <Show when={overview()} fallback={<div class="placeholder-panel">{overviewLoading() ? tr("langs.laravel.analysingTheApplicationEllipsis") : tr("langs.laravel.noLaravelApplicationDetected")}</div>}>
      {(o) => (
        <div class="lv-overview">
          <section class="lv-card">
            <h3>{tr("langs.laravel.application")}</h3>
            <Fact label={tr("common.name")} value={o().appName} />
            <Fact label="Laravel" value={o().version} />
            <Fact label="PHP" value={o().php} mono />
            <Fact label={tr("langs.laravel.environment")} value={o().environment} />
            <Fact label={tr("common.debug2")} value={o().debug === undefined ? null : o().debug ? "on" : "off"} />
            <Fact label="URL" value={o().url} mono />
          </section>
          <section class="lv-card">
            <h3>{tr("common.project")}</h3>
            <Fact label={tr("langs.laravel.routes")} value={o().routes} />
            <Fact label={tr("langs.laravel.models")} value={o().models.length} />
            <Fact label={tr("langs.laravel.artisanCommands")} value={o().commands.length} />
            <Fact label={tr("langs.laravel.views")} value={o().views} />
            <Fact label={tr("langs.laravel.bladeComponents")} value={o().components} />
            <Fact label={tr("langs.laravel.translations")} value={o().translations} />
            <Fact label={tr("langs.laravel.configKeys")} value={o().configKeys} />
            <Fact label={tr("langs.laravel.logFiles")} value={o().logFiles.length} />
          </section>
          <section class="lv-card lv-wide">
            <h3>{tr("langs.laravel.models")}</h3>
            <table class="lv-table">
              <thead>
                <tr>
                  <th>{tr("common.model")}</th>
                  <th>{tr("common.table")}</th>
                  <th>{tr("langs.laravel.attributes")}</th>
                  <th>{tr("langs.laravel.relations")}</th>
                </tr>
              </thead>
              <tbody>
                <For each={o().models} fallback={<tr><td colSpan={4} class="muted">{tr("langs.laravel.noEloquentModels")}</td></tr>}>
                  {(m) => (
                    <tr onClick={() => void reveal(m.location)} title={m.class}>
                      <td class="lv-strong">
                        <Database size={12} class="lv-icon" />
                        {m.name}
                      </td>
                      <td class="mono">{m.table}</td>
                      <td>{m.attributes}</td>
                      <td>{m.relations}</td>
                    </tr>
                  )}
                </For>
              </tbody>
            </table>
          </section>
          <section class="lv-card lv-wide">
            <h3>{tr("langs.laravel.artisanCommands")}</h3>
            <table class="lv-table">
              <tbody>
                <For each={o().commands} fallback={<tr><td class="muted">{tr("langs.laravel.theApplicationDefinesNoCommands")}</td></tr>}>
                  {(c) => (
                    <tr onClick={() => void reveal(c.location)} title={c.signature}>
                      <td class="lv-strong mono">
                        <SquareTerminal size={12} class="lv-icon" />
                        {c.name}
                      </td>
                      <td class="muted">{c.description}</td>
                      <td class="lv-actions">
                        <button
                          class="icon-btn"
                          title={tr("langs.laravel.runEllipsis", { name: c.name })}
                          onClick={(e) => {
                            e.stopPropagation();
                            openRunCommand(`${artisanAlias()} ${c.name} `);
                          }}
                        >
                          <Play size={13} />
                        </button>
                      </td>
                    </tr>
                  )}
                </For>
              </tbody>
            </table>
          </section>
          <section class="lv-card lv-wide">
            <h3>ide.json</h3>
            <Show when={o().ideJson.files.length} fallback={<div class="muted lv-note">{tr("langs.laravel.noIdeJsonInThe")}</div>}>
              <div class="lv-note">
                <For each={o().ideJson.files}>
                  {(f) => (
                    <button class="link-btn mono" onClick={() => void revealLocation(f, { line: 0, character: 0 })}>
                      {f}
                    </button>
                  )}
                </For>
                <span class="muted"> {tr("langs.laravel.completions", { count: o().ideJson.completions })}</span>
              </div>
              <For each={o().ideJson.errors ?? []}>{(e) => <div class="lv-error">{e}</div>}</For>
              <table class="lv-table">
                <tbody>
                  <For each={o().ideJson.codeGenerations}>
                    {(g) => (
                      <tr title={g.files.map((f) => f.name).join(", ")}>
                        <td class="lv-strong">
                          <Wand2 size={12} class="lv-icon" />
                          {g.name}
                        </td>
                        <td class="muted mono">{g.files.map((f) => (f.appNamespace ? `App\\${f.appNamespace}` : "App")).join(", ")}</td>
                        <td class="muted">{g.source}</td>
                        <td class="lv-actions">
                          <button class="pill-btn" onClick={() => void runGeneration(g.id, g.name, g.classSuffix)}>
                            {tr("langs.laravel.generateEllipsis")}
                          </button>
                        </td>
                      </tr>
                    )}
                  </For>
                </tbody>
              </table>
            </Show>
          </section>
        </div>
      )}
    </Show>
  );
}

// ---------------------------------------------------------------- Logs

function FrameLink(props: { frame: LogStackFrame }) {
  const label = () => (props.frame.path ?? props.frame.file ?? "") + (props.frame.line ? `:${props.frame.line}` : "");
  return (
    <Show when={props.frame.path} fallback={<span class="lv-frame-file muted">{label()}</span>}>
      {(p) => (
        <button
          class="link-btn lv-frame-file"
          classList={{ vendor: props.frame.vendor }}
          onClick={(e) => {
            e.stopPropagation();
            void revealLocation(p(), { line: Math.max(0, (props.frame.line ?? 1) - 1), character: 0 });
          }}
        >
          {label()}
        </button>
      )}
    </Show>
  );
}

function Entry(props: { entry: LaravelLogEntry; open: boolean; onToggle: () => void }) {
  const e = () => props.entry;
  const expandable = () => !!(e().exception || e().frames?.length || e().context);
  return (
    <div class="lv-entry" classList={{ open: props.open }} data-level={e().level}>
      <div class="lv-entry-head" onClick={() => expandable() && props.onToggle()}>
        <span class="lv-chevron">
          <Show when={expandable()}>{props.open ? <ChevronDown size={12} /> : <ChevronRight size={12} />}</Show>
        </span>
        <span class={`lv-level lv-level-${e().level}`}>{e().level.toUpperCase()}</span>
        <span class="lv-time mono">{e().time}</span>
        <span class="lv-channel muted">{e().channel}</span>
        <span class="lv-message">
          {e().message}
          <Show when={e().exception}>{(x) => <span class="lv-exception-class"> {x().class}</span>}</Show>
        </span>
        <Show when={e().exception?.frame}>{(f) => <FrameLink frame={f()} />}</Show>
      </div>
      <Show when={props.open}>
        <div class="lv-entry-body">
          <Show when={e().exception}>
            {(x) => (
              <div class="lv-exception">
                <span class="lv-exception-class">{x().class}</span>: {x().message}
              </div>
            )}
          </Show>
          <Show when={e().context}>
            <pre class="lv-context">{e().context}</pre>
          </Show>
          <For each={e().frames ?? []}>
            {(f, i) => (
              <div class="lv-frame" classList={{ vendor: f.vendor }}>
                <span class="lv-frame-index muted">#{i()}</span>
                <FrameLink frame={f} />
                <span class="lv-frame-call mono">{f.call}</span>
              </div>
            )}
          </For>
        </div>
      </Show>
    </div>
  );
}

function LogsTab() {
  const [query, setQuery] = createSignal("");
  const [minLevel, setMinLevel] = createSignal<LogLevel>("debug");
  const [follow, setFollow] = createSignal<boolean>(laravelSetting<boolean>("laravel.logs.follow", true));
  const [open, setOpen] = createSignal<Set<number>>(new Set());
  let scroller: HTMLDivElement | undefined;

  void loadLogs();
  const timer = setInterval(() => {
    if (follow() && !logsLoading()) void loadLogs();
  }, 2000);
  onCleanup(() => clearInterval(timer));

  const shown = createMemo(() => {
    const q = query().trim().toLowerCase();
    return (page()?.entries ?? []).filter(
      (e) =>
        rank(e.level) >= rank(minLevel()) &&
        (!q || `${e.message} ${e.channel} ${e.exception?.class ?? ""} ${e.exception?.message ?? ""} ${e.context ?? ""}`.toLowerCase().includes(q)),
    );
  });
  createEffect(
    on(
      () => shown().length,
      () => {
        if (follow() && scroller) queueMicrotask(() => scroller && (scroller.scrollTop = scroller.scrollHeight));
      },
    ),
  );
  const toggle = (line: number) =>
    setOpen((s) => {
      const n = new Set(s);
      if (n.has(line)) n.delete(line);
      else n.add(line);
      return n;
    });
  return (
    <div class="lv-logs">
      <div class="panel-toolbar lv-toolbar">
        <select class="lv-select" value={logFile()} onChange={(e) => { setLogFile(e.currentTarget.value); void loadLogs(); }} title={tr("langs.laravel.logFile")}>
          <For each={page()?.files ?? []} fallback={<option value="">{tr("langs.laravel.noLogFiles")}</option>}>
            {(f) => <option value={f.path}>{f.path.replace(/^storage\/logs\//, "")} ({size(f.size)})</option>}
          </For>
        </select>
        <select class="lv-select" value={minLevel()} onChange={(e) => setMinLevel(e.currentTarget.value as LogLevel)} title={tr("langs.laravel.minimumLevel")}>
          <For each={LEVELS}>{(l) => <option value={l}>{l === "debug" ? tr("langs.laravel.allLevels") : tr("langs.laravel.andAbove", { value: l[0].toUpperCase(), l: l.slice(1) })}</option>}</For>
        </select>
        <label class="lv-search">
          <Search size={13} />
          <input type="text" placeholder={tr("langs.laravel.searchMessages")} value={query()} onInput={(e) => setQuery(e.currentTarget.value)} spellcheck={false} />
          <Show when={query()}>
            <button class="icon-btn" title={tr("common.clear")} onClick={() => setQuery("")}>
              <X size={11} />
            </button>
          </Show>
        </label>
        <button class="icon-btn" classList={{ active: follow() }} title={tr("langs.laravel.followReloadAndScrollTo")} aria-pressed={follow()} onClick={() => setFollow(!follow())}>
          <ArrowDownToLine size={14} />
        </button>
        <button class="icon-btn" title={tr("common.reload")} onClick={() => void loadLogs()}>
          <RefreshCw size={14} class={logsLoading() ? "laravel-spin" : undefined} />
        </button>
        <span class="lv-count muted">
          {tr("langs.laravel.ofEntries", { count: shown().length, value: page()?.entries.length ?? 0 })}
          <Show when={page()?.truncated}> {tr("langs.laravel.last", { value: size(2 * 1024 * 1024) })}</Show>
        </span>
      </div>
      <div class="lv-entries" ref={scroller}>
        <For each={shown()} fallback={<div class="routes-empty">{page()?.path ? tr("langs.laravel.noEntriesMatch") : tr("langs.laravel.noLogFilesInStorage")}</div>}>
          {(e) => <Entry entry={e} open={open().has(e.line)} onToggle={() => toggle(e.line)} />}
        </For>
      </div>
    </div>
  );
}

export function LaravelView() {
  // The header shows the version and environment from the overview.
  if (!overview() && !overviewLoading()) void loadOverview();
  return (
    <div class="lv-panel">
      <div class="sf-tabs" role="tablist">
        <button role="tab" class="sf-tab" classList={{ active: tab() === "overview" }} onClick={() => setTab("overview")}>
          {tr("langs.laravel.overview")}
        </button>
        <button role="tab" class="sf-tab" classList={{ active: tab() === "logs" }} onClick={() => setTab("logs")}>
          {tr("langs.laravel.logs")}
        </button>
      </div>
      <Switch>
        <Match when={tab() === "overview"}>
          <OverviewTab />
        </Match>
        <Match when={tab() === "logs"}>
          <LogsTab />
        </Match>
      </Switch>
    </div>
  );
}

export function LaravelActions() {
  return (
    <>
      <Show when={overview()}>{(o) => <span class="sf-header-info muted">{tr("langs.laravel.laravel", { value: o().version?.split(".")[0], value2: o().environment ?? "?" })}</span>}</Show>
      <button class="icon-btn" title={tr("langs.laravel.runArtisanCommandEllipsis")} onClick={() => openRunCommand(`${artisanAlias()} `)}>
        <Play size={14} />
      </button>
      <button
        class="icon-btn"
        title={tr("common.refresh")}
        onClick={() => {
          void loadOverview();
          if (tab() === "logs") void loadLogs();
        }}
      >
        <RefreshCw size={14} class={overviewLoading() ? "laravel-spin" : undefined} />
      </button>
    </>
  );
}
