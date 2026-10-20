/**
 * Details of the selected Services node: Dashboard (sections and tables from the backend),
 * Log (followed container / pod logs, or compose and build output), Attached Console
 * (`docker attach` in a terminal) and Inspection (`docker inspect` / the Kubernetes object).
 */
import { createEffect, createResource, createSignal, For, Match, on, onCleanup, onMount, Show, Switch } from "solid-js";
import { ArrowDownToLine, ExternalLink, WrapText } from "lucide-solid";
import { Console } from "@/features/run/Console";
import { attachSession, disposeSession, fitSession } from "@/features/terminal/sessions";
import { servicesApi, logKey, runKey, type DetailsTab, type ServiceDetails } from "@/ipc/services";
import { consoleFor, setTab, tab } from "./store";
import { NodeIconView } from "./icons";
import "@xterm/xterm/css/xterm.css";
import "@/features/run/run.css";

const TAB_TITLES: Record<DetailsTab, string> = {
  dashboard: "Dashboard",
  log: "Log",
  console: "Attached Console",
  inspection: "Inspection",
};

/** Log tabs of containers, compose services and pods follow the logs; others show output. */
const followsLogs = (d: ServiceDetails) => ["container", "composeService", "pod"].includes(d.target.kind);
const consoleKey = (d: ServiceDetails) => (followsLogs(d) ? logKey(d.nodeId) : runKey(d.nodeId));

const following = new Set<string>();

function Dashboard(props: { d: ServiceDetails }) {
  return (
    <div class="svc-dashboard">
      <Show when={props.d.message}>
        <div class="svc-message">
          <For each={props.d.message!.split("\n")}>{(l) => <div>{l}</div>}</For>
        </div>
      </Show>
      <For each={props.d.sections}>
        {(s) => (
          <section class="svc-section" data-section={s.title}>
            <h4 class="svc-section-title">{s.title}</h4>
            <Show when={s.rows?.length}>
              <div class="svc-rows">
                <For each={s.rows}>
                  {(r) => (
                    <>
                      <div class="svc-row-label">{r.label}</div>
                      <div class="svc-row-value">
                        <Show when={r.status}>
                          <span class={`svc-dot ${r.status}`} />
                        </Show>
                        <span class="svc-value-text">{r.value}</span>
                        <Show when={r.url}>
                          <a class="svc-link" href={r.url} target="_blank" rel="noopener" title={`Open ${r.url}`}>
                            {r.url}
                            <ExternalLink size={11} />
                          </a>
                        </Show>
                      </div>
                    </>
                  )}
                </For>
              </div>
            </Show>
            <Show when={s.table}>
              {(t) => (
                <table class="svc-table">
                  <thead>
                    <tr>
                      <For each={t().columns}>{(c) => <th>{c}</th>}</For>
                    </tr>
                  </thead>
                  <tbody>
                    <For each={t().rows}>
                      {(row, i) => (
                        <tr>
                          <For each={row}>
                            {(cell, j) => (
                              <td>
                                <Show when={j() === 0 && t().status?.[i()]}>
                                  <span class={`svc-dot ${t().status![i()]}`} />
                                </Show>
                                {cell}
                              </td>
                            )}
                          </For>
                        </tr>
                      )}
                    </For>
                  </tbody>
                </table>
              )}
            </Show>
          </section>
        )}
      </For>
    </div>
  );
}

function LogTab(props: { d: ServiceDetails }) {
  const key = consoleKey(props.d);
  const c = consoleFor(key);
  const [wrap, setWrap] = createSignal(false);
  const [follow, setFollow] = createSignal(true);
  onMount(() => {
    if (followsLogs(props.d) && !following.has(key)) {
      following.add(key);
      void servicesApi.logsStart(props.d.nodeId).catch((err) => {
        following.delete(key);
        c.lines.push({ segments: [{ text: String(err).replace(/^Error: /, ""), style: { fg: "var(--danger)" } }] });
        c.bump((n) => n + 1);
      });
    }
  });
  createEffect(
    on(c.ended, (ended) => {
      if (ended) following.delete(key);
    }),
  );
  return (
    <div class="svc-log">
      <div class="svc-log-tools">
        <button class="icon-btn" classList={{ active: wrap() }} title="Soft-Wrap" onClick={() => setWrap(!wrap())}>
          <WrapText size={14} />
        </button>
        <button class="icon-btn" classList={{ active: follow() }} title="Scroll to the End" onClick={() => setFollow(!follow())}>
          <ArrowDownToLine size={14} />
        </button>
      </div>
      <Console
        lines={() => c.lines}
        version={c.version}
        softWrap={wrap()}
        scrollToEnd={follow()}
        emptyText={followsLogs(props.d) ? "No log output yet." : "Output of Up, Down, builds and pulls appears here."}
      />
    </div>
  );
}

function AttachedConsole(props: { d: ServiceDetails }) {
  let host!: HTMLDivElement;
  const id = `services-attach-${props.d.nodeId.replace(/[^a-zA-Z0-9-]/g, "_")}`;
  onMount(async () => {
    try {
      const cmd = await servicesApi.terminal(props.d.nodeId, true);
      attachSession(id, host, undefined, { program: cmd.program, args: cmd.args, env: cmd.env });
      const ro = new ResizeObserver(() => fitSession(id));
      ro.observe(host);
      onCleanup(() => ro.disconnect());
    } catch (err) {
      host.textContent = String(err).replace(/^Error: /, "");
    }
  });
  onCleanup(() => disposeSession(id));
  return <div class="svc-attach" ref={host} />;
}

function Inspection(props: { d: ServiceDetails }) {
  const [text] = createResource(
    () => props.d.nodeId,
    (id) => servicesApi.inspect(id).catch((err) => `// ${String(err).replace(/^Error: /, "")}`),
  );
  return (
    <pre class="svc-inspect" tabIndex={0}>
      <Show when={text()} fallback={<span class="muted">Loading…</span>}>
        {text()}
      </Show>
    </pre>
  );
}

export function DetailsPane(props: { details: () => ServiceDetails }) {
  const d = props.details;
  // Keep the tab when the node changes if it has it, else show the Dashboard.
  createEffect(
    on(
      () => d().tabs,
      (tabs) => {
        if (!tabs.includes(tab())) setTab("dashboard");
      },
    ),
  );
  return (
    <div class="svc-details" data-node={d().nodeId}>
      <header class="svc-details-head">
        <NodeIconView icon={d().icon} status={d().status} size={15} />
        <span class="svc-details-title">{d().title}</span>
        <Show when={d().subtitle}>
          <span class="svc-details-sub">{d().subtitle}</span>
        </Show>
      </header>
      <div class="svc-tabs" role="tablist">
        <For each={d().tabs}>
          {(t) => (
            <button class="svc-tab" classList={{ active: tab() === t }} role="tab" aria-selected={tab() === t} data-tab={t} onClick={() => setTab(t)}>
              {TAB_TITLES[t]}
            </button>
          )}
        </For>
      </div>
      <div class="svc-tab-body">
        <Switch>
          <Match when={tab() === "dashboard"}>
            <Dashboard d={d()} />
          </Match>
          <Match when={tab() === "log" && d().tabs.includes("log")}>
            <LogTab d={d()} />
          </Match>
          <Match when={tab() === "console" && d().tabs.includes("console")}>
            <AttachedConsole d={d()} />
          </Match>
          <Match when={tab() === "inspection" && d().tabs.includes("inspection")}>
            <Inspection d={d()} />
          </Match>
        </Switch>
      </div>
    </div>
  );
}
