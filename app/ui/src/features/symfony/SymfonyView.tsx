/**
 * Symfony tool window: Routes, Services and Parameters tabs plus one tab per console run,
 * like PHPStorm's Endpoints and Run tool windows combined.
 */
import { createEffect, createMemo, createSignal, For, Match, on, Show, Switch } from "solid-js";
import { Globe, Lock, Play, RefreshCw, RotateCw, Search, Square, SquareTerminal, X } from "lucide-solid";
import type { SourceLocation } from "@/ipc/symfony";
import { revealSymbol } from "@/features/php/editorViews";
import { activeTab, closeRun, rerun, runs, setActiveTab, stopRun, type ConsoleRun } from "./console";
import { openConsolePopup } from "./ConsolePopup";
import { loadSymfony, loading, parameters, routes, services, shortVersion, symfonyInfo } from "./store";

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
      <FilterBar value={filter()} onInput={setFilter} placeholder="Filter by name, path or controller" count={`${shown().length} routes`} />
      <div class="sf-table-wrap">
        <table class="sf-table sf-routes">
          <thead>
            <tr>
              <th style={{ width: "22%" }}>Name</th>
              <th style={{ width: "10%" }}>Method</th>
              <th style={{ width: "28%" }}>Path</th>
              <th>Controller</th>
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
      <FilterBar value={filter()} onInput={setFilter} placeholder="Filter by id, class or tag" count={`${shown().length} services`}>
        <label class="sf-check">
          <input type="checkbox" checked={showContainer()} onChange={(e) => setShowContainer(e.currentTarget.checked)} />
          Show framework services
        </label>
      </FilterBar>
      <div class="sf-table-wrap">
        <table class="sf-table sf-services">
          <thead>
            <tr>
              <th style={{ width: "34%" }}>Service ID</th>
              <th style={{ width: "36%" }}>Class</th>
              <th>Tags</th>
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
                      <span class="sf-flag" title="Public service">public</span>
                    </Show>
                    <Show when={!s.public && s.source !== "container"}>
                      <Lock size={11} class="sf-private" aria-label="Private" />
                    </Show>
                  </td>
                  <td class="muted sf-ellipsis" title={s.alias ? `alias for ${s.alias}` : s.class}>
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
      <FilterBar value={filter()} onInput={setFilter} placeholder="Filter parameters" count={`${shown().length} parameters`} />
      <div class="sf-table-wrap">
        <table class="sf-table">
          <thead>
            <tr>
              <th style={{ width: "36%" }}>Name</th>
              <th>Value</th>
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
        <button class="icon-btn" title="Rerun" onClick={() => rerun(props.run)} disabled={props.run.running}>
          <RotateCw size={14} />
        </button>
        <button class="icon-btn" title="Stop" onClick={() => stopRun(props.run.id)} disabled={!props.run.running}>
          <Square size={13} class={props.run.running ? "sf-stop" : ""} />
        </button>
      </div>
      <div class="sf-run-output" ref={out}>
        <div class="sf-run-cmd">{props.run.commandLine}</div>
        <For each={props.run.lines}>{(l) => <div classList={{ "sf-run-line": true, stderr: l.stderr }}>{l.text || " "}</div>}</For>
        <Show when={!props.run.running}>
          <div class="sf-run-exit" classList={{ failed: props.run.exitCode !== 0 }}>
            {"\n"}Process finished with exit code {props.run.exitCode ?? "?"}
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
                title="Close"
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
        fallback={<div class="placeholder-panel">{loading() ? "Indexing Symfony project…" : "No Symfony project detected."}</div>}
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
        {(i) => <span class="sf-header-info muted">Symfony {shortVersion(i().version)} · {i().environment}</span>}
      </Show>
      <button class="icon-btn" title="Run Console Command…" onClick={() => openConsolePopup()}>
        <Play size={14} />
      </button>
      <button class="icon-btn" classList={{ spinning: loading() }} title="Refresh" onClick={() => void loadSymfony(true)}>
        <RefreshCw size={14} />
      </button>
    </>
  );
}
