/**
 * Details of the selected Services node: Dashboard (sections and tables from the backend,
 * a pod's port forwards), Log (followed container / pod / deployment logs, or compose and
 * build output), Attached Console (`docker attach` / `kubectl attach` in a terminal) and
 * Inspection (`docker inspect` / the Kubernetes object). Pods with several containers get a
 * container picker for Log, Exec and the Attached Console.
 */
import { createEffect, createMemo, createResource, createSignal, For, Match, on, onCleanup, onMount, Show, Switch } from "solid-js";
import { ArrowDownToLine, ArrowRightLeft, ExternalLink, Square, WrapText } from "lucide-solid";
import { Console } from "@/features/run/Console";
import { attachSession, disposeSession, fitSession } from "@/features/terminal/sessions";
import { servicesApi, logKey, runKey, type DetailsTab, type ServiceDetails } from "@/ipc/services";
import { consoleFor, containerOf, pickContainer, setForwardDialog, setTab, stopForward, tab } from "./store";
import { NodeIconView } from "./icons";
import { t as tr } from "@/i18n";
import "@xterm/xterm/css/xterm.css";
import "@/features/run/run.css";

const TAB_TITLES: Record<DetailsTab, string> = {
  dashboard: "Dashboard",
  log: "Log",
  console: "Attached Console",
  inspection: "Inspection",
};

/** Log tabs of containers, compose services, pods and deployments follow the logs; others
 * show output. */
const followsLogs = (d: ServiceDetails) => ["container", "composeService", "pod", "deployment"].includes(d.target.kind);
const consoleKey = (d: ServiceDetails) => (followsLogs(d) ? logKey(d.nodeId) : runKey(d.nodeId));

/** Followed log streams and the container they follow ("" for none). */
const following = new Map<string, string>();

/** Port forwards of a pod (with Forward Ports…) or of all pods on the Kubernetes nodes. */
function PortForwards(props: { d: ServiceDetails }) {
  const pod = () => props.d.target.kind === "pod";
  return (
    <section class="svc-section svc-forwards" data-section="Port Forwarding">
      <h4 class="svc-section-title">
        {tr("tools.services.portForwarding")}
        <Show when={pod()}>
          <button class="svc-title-btn" data-testid="forward-ports" onClick={() => setForwardDialog(props.d)}>
            <ArrowRightLeft size={12} /> {tr("tools.services.forwardPortsEllipsis")}
          </button>
        </Show>
      </h4>
      <Show when={props.d.portForwards?.length} fallback={<div class="muted svc-forwards-empty">{tr("tools.services.noForwardedPorts")}</div>}>
        <table class="svc-table svc-forward-table">
          <thead>
            <tr>
              <Show when={!pod()}>
                <th>{tr("tools.services.pod")}</th>
              </Show>
              <th>{tr("tools.services.localAddress")}</th>
              <th>{tr("tools.services.podPort")}</th>
              <th>{tr("tools.services.connections")}</th>
              <th>{tr("common.status")}</th>
              <th />
            </tr>
          </thead>
          <tbody>
            <For each={props.d.portForwards}>
              {(f) => (
                <tr data-forward={f.id}>
                  <Show when={!pod()}>
                    <td>
                      {f.namespace}/{f.pod}
                    </td>
                  </Show>
                  <td>
                    <span class={`svc-dot ${f.error ? "error" : "running"}`} />
                    <a class="svc-link" href={`http://localhost:${f.localPort}`} target="_blank" rel="noopener" title={tr("tools.services.openHttpLocalhost", { localPort: f.localPort })}>
                      {f.address}:{f.localPort}
                    </a>
                  </td>
                  <td>{f.remotePort}</td>
                  <td>{f.connections}</td>
                  <td classList={{ "svc-forward-error": !!f.error }} title={f.error}>
                    {f.error ?? (f.via ? tr("tools.services.forwardingVia", { via: f.via }) : tr("tools.services.forwarding"))}
                  </td>
                  <td class="svc-cell-actions">
                    <button class="icon-btn svc-stop-forward" title={tr("tools.services.stopPortForwarding")} data-testid="stop-forward" onClick={() => void stopForward(f.id)}>
                      <Square size={11} stroke-width={0} fill="currentColor" />
                    </button>
                  </td>
                </tr>
              )}
            </For>
          </tbody>
        </table>
      </Show>
    </section>
  );
}

/** Container combo of a pod (Log, Exec and Attached Console use the picked one). */
function ContainerPicker(props: { d: ServiceDetails }) {
  const list = () => props.d.containers ?? [];
  return (
    <label class="svc-container-picker" title={tr("tools.services.containerForLogExecAnd")}>
      {tr("tools.services.containerLabel")}
      <select data-testid="container-picker" value={containerOf(props.d) ?? ""} onChange={(e) => pickContainer(props.d.nodeId, e.currentTarget.value)}>
        <For each={list()}>
          {(c) => (
            <option value={c.name} disabled={c.init && tab() === "console"}>
              {c.name}
              {c.init ? tr("tools.services.init") : c.default && list().length > 1 ? tr("tools.services.default") : ""}
            </option>
          )}
        </For>
      </select>
    </label>
  );
}

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
      <Show when={props.d.target.kind === "pod" || props.d.portForwards?.length}>
        <PortForwards d={props.d} />
      </Show>
    </div>
  );
}

function LogTab(props: { d: ServiceDetails; container?: string }) {
  const key = consoleKey(props.d);
  const c = consoleFor(key);
  const [wrap, setWrap] = createSignal(false);
  const [follow, setFollow] = createSignal(true);
  onMount(() => {
    const container = props.container ?? "";
    if (followsLogs(props.d) && following.get(key) !== container) {
      following.set(key, container);
      void servicesApi.logsStart(props.d.nodeId, props.container).catch((err) => {
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
        <button class="icon-btn" classList={{ active: wrap() }} title={tr("tools.services.softWrap")} onClick={() => setWrap(!wrap())}>
          <WrapText size={14} />
        </button>
        <button class="icon-btn" classList={{ active: follow() }} title={tr("common.scrollToTheEnd")} onClick={() => setFollow(!follow())}>
          <ArrowDownToLine size={14} />
        </button>
      </div>
      <Console
        lines={() => c.lines}
        version={c.version}
        softWrap={wrap()}
        scrollToEnd={follow()}
        emptyText={
          props.d.target.kind === "deployment"
            ? tr("tools.services.noLogOutputFromThe")
            : followsLogs(props.d)
              ? tr("tools.services.noLogOutputYet")
              : tr("tools.services.outputOfUpDownBuilds")
        }
      />
    </div>
  );
}

function AttachedConsole(props: { d: ServiceDetails; container?: string }) {
  let host!: HTMLDivElement;
  const id = `services-attach-${`${props.d.nodeId}-${props.container ?? ""}`.replace(/[^a-zA-Z0-9-]/g, "_")}`;
  onMount(async () => {
    try {
      const cmd = await servicesApi.terminal(props.d.nodeId, true, props.container);
      attachSession(id, host, undefined, { program: cmd.program, args: cmd.args, env: cmd.env, local: !cmd.onHost });
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
      <Show when={text()} fallback={<span class="muted">{tr("common.loadingEllipsis")}</span>}>
        {text()}
      </Show>
    </pre>
  );
}

/** Key of "no container picked" (Kubernetes names never start with `-`). */
const NO_CONTAINER = "-";

export function DetailsPane(props: { details: () => ServiceDetails }) {
  const d = props.details;
  // Log and Attached Console restart only when the picked container changes.
  const containerKey = createMemo(() => containerOf(d()) ?? NO_CONTAINER);
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
        <Show when={(d().containers?.length ?? 0) > 1}>
          <ContainerPicker d={d()} />
        </Show>
      </div>
      <div class="svc-tab-body">
        <Switch>
          <Match when={tab() === "dashboard"}>
            <Dashboard d={d()} />
          </Match>
          <Match when={tab() === "log" && d().tabs.includes("log")}>
            <Show when={containerKey()} keyed>
              {(c) => <LogTab d={d()} container={c === NO_CONTAINER ? undefined : c} />}
            </Show>
          </Match>
          <Match when={tab() === "console" && d().tabs.includes("console")}>
            <Show when={containerKey()} keyed>
              {(c) => <AttachedConsole d={d()} container={c === NO_CONTAINER ? undefined : c} />}
            </Show>
          </Match>
          <Match when={tab() === "inspection" && d().tabs.includes("inspection")}>
            <Inspection d={d()} />
          </Match>
        </Switch>
      </div>
    </div>
  );
}
