/**
 * Symfony tool window: Routes, Services, Parameters, Translations and Events tabs plus one
 * tab per console run,
 * like PHPStorm's Endpoints and Run tool windows combined.
 */
import { createEffect, createMemo, createSignal, For, Match, on, Show, Switch } from "solid-js";
import { CircleAlert, Globe, Languages, Lock, Play, Radio, RefreshCw, RotateCw, Search, Square, SquareTerminal, X } from "lucide-solid";
import type { SourceLocation } from "@/ipc/symfony";
import { revealSymbol } from "@/features/php/editorViews";
import { activeTab, closeRun, rerun, runs, setActiveTab, stopRun, type ConsoleRun } from "./console";
import { openConsolePopup } from "./ConsolePopup";
import { listeners, loadSymfony, loading, parameters, routes, services, shortVersion, symfonyInfo, translations } from "./store";
import { t as tr } from "@/i18n";

export const reveal = (loc?: SourceLocation) => {
  if (loc) void revealSymbol(loc.path, { line: loc.line });
};

const short = (fqn?: string) => (fqn ? fqn.split("\\").pop() : "");

/** `App\Controller\OrderController::show` -> `OrderController::show`. */
const shortController = (c?: string) => {
  if (!c) return "";
  const [cls, method] = c.split("::");
  return method ? `${short(cls)}::${method}` : (short(cls) ?? c);
};

function matches(filter: string, ...values: (string | undefined)[]): boolean {
  const f = filter.trim().toLowerCase();
  return !f || values.some((v) => v?.toLowerCase().includes(f));
}

export function MethodBadges(props: { methods?: string[] }) {
  const list = () => (props.methods?.length ? props.methods : ["ANY"]);
  return (
    <span class="sf-methods">
      <For each={list()}>{(m) => <span class={`sf-method sf-method-${m.toLowerCase()}`}>{m}</span>}</For>
    </span>
  );
}

function FilterBar(props: { value: string; onInput: (v: string) => void; placeholder: string; count: string; children?: unknown }) {
  return (
    <div class="panel-toolbar sf-toolbar">
      <label class="sf-search">
        <Search size={13} />
        <input
          type="text"
          placeholder={props.placeholder}
          value={props.value}
          onInput={(e) => props.onInput(e.currentTarget.value)}
          spellcheck={false}
        />
      </label>
      {props.children as never}
      <span class="sf-count muted">{props.count}</span>
    </div>
  );
}

function RoutesTab() {
  const [filter, setFilter] = createSignal("");
  const [selected, setSelected] = createSignal<string | null>(null);
  const shown = createMemo(() => routes().filter((r) => matches(filter(), r.name, r.path, r.controller, r.methods?.join(" "))));
  return (
    <div class="sf-tab-body">
      <FilterBar value={filter()} onInput={setFilter} placeholder={tr("langs.symfony.filterByNamePathOr")} count={`${shown().length} routes`} />
      <div class="sf-table-wrap">
        <table class="sf-table sf-routes">
          <thead>
            <tr>
              <th style={{ width: "22%" }}>{tr("common.name")}</th>
              <th style={{ width: "10%" }}>{tr("common.method")}</th>
              <th style={{ width: "28%" }}>{tr("common.path")}</th>
              <th>{tr("langs.symfony.controller")}</th>
            </tr>
          </thead>
          <tbody>
            <For each={shown()}>
              {(r) => (
                <tr
                  classList={{ selected: selected() === r.name }}
                  data-route={r.name}
                  onClick={() => setSelected(r.name)}
                  onDblClick={() => reveal(r.location)}
                  title={r.controller ?? ""}
                >
                  <td class="sf-name">
                    <Globe size={13} class="sf-row-icon sf-icon-route" />
                    {r.name}
                  </td>
                  <td>
                    <MethodBadges methods={r.methods} />
                  </td>
                  <td class="sf-path">{r.path}</td>
                  <td class="muted sf-ellipsis">
                    {shortController(r.controller)}
                    <Show when={r.source === "yaml"}>
                      <span class="sf-source">yaml</span>
                    </Show>
                  </td>
                </tr>
              )}
            </For>
          </tbody>
        </table>
      </div>
    </div>
  );
}

function ServicesTab() {
  const [filter, setFilter] = createSignal("");
  const [showContainer, setShowContainer] = createSignal(true);
  const [selected, setSelected] = createSignal<string | null>(null);
  const shown = createMemo(() =>
    services().filter((s) => (showContainer() || s.source !== "container") && matches(filter(), s.id, s.class, s.alias, s.tags?.join(" "))),
  );
  return (
    <div class="sf-tab-body">
      <FilterBar value={filter()} onInput={setFilter} placeholder={tr("langs.symfony.filterByIdClassOr")} count={`${shown().length} services`}>
        <label class="sf-check">
          <input type="checkbox" checked={showContainer()} onChange={(e) => setShowContainer(e.currentTarget.checked)} />
          {tr("langs.symfony.showFrameworkServices")}
        </label>
      </FilterBar>
      <div class="sf-table-wrap">
        <table class="sf-table sf-services">
          <thead>
            <tr>
              <th style={{ width: "34%" }}>{tr("langs.symfony.serviceId")}</th>
              <th style={{ width: "36%" }}>{tr("langs.symfony.class")}</th>
              <th>{tr("langs.symfony.tags")}</th>
            </tr>
          </thead>
          <tbody>
            <For each={shown()}>
              {(s) => (
                <tr
                  classList={{ selected: selected() === s.id, vendor: s.source === "container" }}
                  data-service={s.id}
                  onClick={() => setSelected(s.id)}
                  onDblClick={() => reveal(s.location)}
                >
                  <td class="sf-name sf-ellipsis" title={s.id}>
                    <span class={`sf-svc-dot ${s.source}`} />
                    {s.id}
                    <Show when={s.public}>
                      <span class="sf-flag" title={tr("langs.symfony.publicService")}>public</span>
                    </Show>
                    <Show when={!s.public && s.source !== "container"}>
                      <Lock size={11} class="sf-private" aria-label={tr("langs.symfony.private")} />
                    </Show>
                  </td>
                  <td class="muted sf-ellipsis" title={s.alias ? tr("langs.symfony.aliasFor", { alias: s.alias }) : s.class}>
                    {s.alias ? `→ ${s.alias}` : s.class}
                  </td>
                  <td class="sf-ellipsis">
                    <For each={s.tags ?? []}>{(t) => <span class="sf-tag">{t}</span>}</For>
                    <Show when={s.autowire && s.source !== "container"}>
                      <span class="sf-tag sf-tag-soft">autowired</span>
                    </Show>
                  </td>
                </tr>
              )}
            </For>
          </tbody>
        </table>
      </div>
    </div>
  );
}

function ParametersTab() {
  const [filter, setFilter] = createSignal("");
  const shown = createMemo(() => parameters().filter((p) => matches(filter(), p.name, p.value)));
  return (
    <div class="sf-tab-body">
      <FilterBar value={filter()} onInput={setFilter} placeholder={tr("langs.symfony.filterParameters")} count={`${shown().length} parameters`} />
      <div class="sf-table-wrap">
        <table class="sf-table">
          <thead>
            <tr>
              <th style={{ width: "36%" }}>{tr("common.name")}</th>
              <th>{tr("common.value")}</th>
            </tr>
          </thead>
          <tbody>
            <For each={shown()}>
              {(p) => (
                <tr onDblClick={() => reveal(p.location)}>
                  <td class="sf-name">{p.name}</td>
                  <td class="sf-path sf-ellipsis">{p.value}</td>
                </tr>
              )}
            </For>
          </tbody>
        </table>
      </div>
    </div>
  );
}

function TranslationsTab() {
  const [filter, setFilter] = createSignal("");
  const [domain, setDomain] = createSignal("");
  const [missingOnly, setMissingOnly] = createSignal(false);
  const table = () => translations();
  const locales = () => table()?.locales ?? [];
  const shown = createMemo(() =>
    (table()?.rows ?? []).filter(
      (r) =>
        (!domain() || r.domain === domain()) &&
        (!missingOnly() || (r.missing?.length ?? 0) > 0) &&
        matches(filter(), r.key, ...Object.values(r.values)),
    ),
  );
  return (
    <div class="sf-tab-body">
      <FilterBar value={filter()} onInput={setFilter} placeholder={tr("langs.symfony.filterByKeyOrText")} count={`${shown().length} keys`}>
        <select class="sf-select" value={domain()} onChange={(e) => setDomain(e.currentTarget.value)} title={tr("langs.symfony.domain")}>
          <option value="">{tr("langs.symfony.allDomains")}</option>
          <For each={table()?.domains ?? []}>{(d) => <option value={d}>{d}</option>}</For>
        </select>
        <label class="sf-check">
          <input type="checkbox" checked={missingOnly()} onChange={(e) => setMissingOnly(e.currentTarget.checked)} />
          {tr("langs.symfony.onlyMissing")}
        </label>
      </FilterBar>
      <div class="sf-table-wrap">
        <table class="sf-table sf-translations">
          <thead>
            <tr>
              <th style={{ width: "26%" }}>{tr("langs.symfony.key")}</th>
              <th style={{ width: "10%" }}>{tr("langs.symfony.domain")}</th>
              <For each={locales()}>
                {(l) => (
                  <th>
                    {l}
                    <Show when={l === table()?.defaultLocale}>
                      <span class="sf-source">default</span>
                    </Show>
                  </th>
                )}
              </For>
            </tr>
          </thead>
          <tbody>
            <For each={shown()}>
              {(r) => (
                <tr data-key={r.key} onDblClick={() => reveal(r.locations[table()?.defaultLocale ?? ""] ?? Object.values(r.locations)[0])}>
                  <td class="sf-name sf-ellipsis" title={r.key}>
                    <Languages size={13} class="sf-row-icon sf-icon-translation" />
                    {r.key}
                  </td>
                  <td class="muted">{r.domain}</td>
                  <For each={locales()}>
                    {(l) => (
                      <td class="sf-ellipsis" title={r.values[l] ?? ""} onDblClick={(e) => {
                        const loc = r.locations[l];
                        if (loc) {
                          e.stopPropagation();
                          reveal(loc);
                        }
                      }}>
                        <Show when={r.values[l] !== undefined} fallback={
                          <Show when={r.missing?.includes(l)}>
                            <span class="sf-missing"><CircleAlert size={12} /> missing</span>
                          </Show>
                        }>
                          {r.values[l]}
                        </Show>
                      </td>
                    )}
                  </For>
                </tr>
              )}
            </For>
          </tbody>
        </table>
      </div>
    </div>
  );
}

const SOURCE_LABEL = { subscriber: "subscriber", attribute: "#[AsEventListener]", yaml: "services.yaml" } as const;

function EventsTab() {
  const [filter, setFilter] = createSignal("");
  const shown = createMemo(() => listeners().filter((l) => matches(filter(), l.event, l.class, l.method, l.eventClass)));
  return (
    <div class="sf-tab-body">
      <FilterBar value={filter()} onInput={setFilter} placeholder={tr("langs.symfony.filterByEventOrListener")} count={`${shown().length} listeners`} />
      <div class="sf-table-wrap">
        <table class="sf-table sf-events">
          <thead>
            <tr>
              <th style={{ width: "30%" }}>{tr("langs.symfony.event")}</th>
              <th style={{ width: "36%" }}>{tr("langs.symfony.listener")}</th>
              <th style={{ width: "10%" }}>{tr("langs.symfony.priority")}</th>
              <th>{tr("langs.symfony.registeredBy")}</th>
            </tr>
          </thead>
          <tbody>
            <For each={shown()}>
              {(l) => (
                <tr onDblClick={() => reveal(l.location)} title={l.eventClass ?? l.event}>
                  <td class="sf-name sf-ellipsis">
                    <Radio size={13} class="sf-row-icon sf-icon-event" />
                    {l.event.includes("\\") ? short(l.event) : l.event}
                  </td>
                  <td class="sf-ellipsis">
                    {short(l.class)}::{l.method}
                  </td>
                  <td class="muted">{l.priority}</td>
                  <td class="muted sf-ellipsis">
                    <span class="sf-source" onClick={() => reveal(l.declaredAt)}>{SOURCE_LABEL[l.source]}</span>
                  </td>
                </tr>
              )}
            </For>
          </tbody>
        </table>
      </div>
    </div>
  );
}

function RunTab(props: { run: ConsoleRun }) {
  let out!: HTMLDivElement;
  createEffect(
    on(
      () => props.run.lines.length,
      () => queueMicrotask(() => (out.scrollTop = out.scrollHeight)),
    ),
  );
  return (
    <div class="sf-run">
      <div class="sf-run-actions">
        <button class="icon-btn" title={tr("langs.symfony.rerun")} onClick={() => rerun(props.run)} disabled={props.run.running}>
          <RotateCw size={14} />
        </button>
        <button class="icon-btn" title={tr("common.stop")} onClick={() => stopRun(props.run.id)} disabled={!props.run.running}>
          <Square size={13} class={props.run.running ? "sf-stop" : ""} />
        </button>
      </div>
      <div class="sf-run-output" ref={out}>
        <div class="sf-run-cmd">{props.run.commandLine}</div>
        <For each={props.run.lines}>{(l) => <div classList={{ "sf-run-line": true, stderr: l.stderr }}>{l.text || " "}</div>}</For>
        <Show when={!props.run.running}>
          <div class="sf-run-exit" classList={{ failed: props.run.exitCode !== 0 }}>
            {tr("langs.symfony.processFinishedWithExitCode", { value: props.run.exitCode ?? "?" })}
          </div>
        </Show>
      </div>
    </div>
  );
}

const TABS = [
  ["routes", "Routes"],
  ["services", "Services"],
  ["parameters", "Parameters"],
  ["translations", "Translations"],
  ["events", "Events"],
] as const;

export function SymfonyView() {
  const run = () => runs().find((r) => r.id === activeTab());
  return (
    <div class="sf-panel">
      <div class="sf-tabs" role="tablist">
        <For each={TABS}>
          {([id, title]) => (
            <button role="tab" class="sf-tab" classList={{ active: activeTab() === id }} onClick={() => setActiveTab(id)}>
              {title}
            </button>
          )}
        </For>
        <For each={runs()}>
          {(r) => (
            <span class="sf-tab sf-run-tab" classList={{ active: activeTab() === r.id }} onClick={() => setActiveTab(r.id)} title={r.commandLine}>
              <SquareTerminal size={12} class={r.running ? "sf-running" : ""} />
              {r.command}
              <button
                class="sf-tab-close"
                title={tr("common.close")}
                onClick={(e) => {
                  e.stopPropagation();
                  closeRun(r.id);
                }}
              >
                <X size={11} />
              </button>
            </span>
          )}
        </For>
      </div>
      <Show
        when={symfonyInfo()}
        fallback={<div class="placeholder-panel">{loading() ? tr("langs.symfony.indexingSymfonyProjectEllipsis") : tr("langs.symfony.noSymfonyProjectDetected")}</div>}
      >
        <Switch>
          <Match when={activeTab() === "routes"}>
            <RoutesTab />
          </Match>
          <Match when={activeTab() === "services"}>
            <ServicesTab />
          </Match>
          <Match when={activeTab() === "parameters"}>
            <ParametersTab />
          </Match>
          <Match when={activeTab() === "translations"}>
            <TranslationsTab />
          </Match>
          <Match when={activeTab() === "events"}>
            <EventsTab />
          </Match>
          <Match when={run()}>{(r) => <RunTab run={r()} />}</Match>
        </Switch>
      </Show>
    </div>
  );
}

export function SymfonyActions() {
  return (
    <>
      <Show when={symfonyInfo()}>
        {(i) => <span class="sf-header-info muted">{tr("langs.symfony.symfony", { value: shortVersion(i().version), environment: i().environment })}</span>}
      </Show>
      <button class="icon-btn" title={tr("langs.symfony.runConsoleCommandEllipsis")} onClick={() => openConsolePopup()}>
        <Play size={14} />
      </button>
      <button class="icon-btn" classList={{ spinning: loading() }} title={tr("common.refresh")} onClick={() => void loadSymfony(true)}>
        <RefreshCw size={14} />
      </button>
    </>
  );
}
