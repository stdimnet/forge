/**
 * .NET Aspire, like Rider's Aspire plugin: AppHost projects and the resources their
 * `Program.cs` declares (read in Rust, `dotnet_aspire_hosts`) in the Services tool window,
 * the ".NET Aspire" run configuration type (`dotnet run` of the AppHost), and the dashboard
 * link from the run's output (`dotnet_run_output`): a balloon, the Services details and
 * **Open Aspire Dashboard**.
 */
import { createEffect, createResource, createRoot, createSignal, on } from "solid-js";
import { Boxes } from "lucide-solid";
import { dotnetWebApi, type AppHost, type AspireResource } from "@/ipc/dotnetWeb";
import { runApi, type RunConfiguration, type RunEvent } from "@/ipc/run";
import type { NodeIcon, ServiceDetails, ServiceNode } from "@/ipc/services";
import { registerRunConfigurationType } from "@/features/run/types";
import { registerServicesProvider } from "@/features/services/providers";
import { bumpDetails } from "@/features/services/store";
import { allConfigurations, useTemporary } from "@/features/run/configStore";
import { launch, openInBrowser, runningFor, stopConfiguration } from "@/features/run/runStore";
import { getSetting } from "@/state/settings";
import { dotnetActive, dotnetWorkspace, PLUGIN, projectByPath, showDotnetBalloon } from "../state";

const [hosts, { refetch }] = createRoot(() => createResource(
  () => (dotnetActive() ? dotnetWorkspace() : null),
  () => dotnetWebApi.aspireHosts().catch(() => [] as AppHost[]),
));
export const aspireActive = () => dotnetActive() && (hosts()?.length ?? 0) > 0;
export const refreshAspire = () => void refetch();

/** Dashboard URL and start of each running AppHost, by project path. */
const [running, setRunning] = createSignal<Record<string, { dashboardUrl?: string; started: boolean }>>({});

export const dashboardUrl = (project?: string) => {
  const r = running();
  return project ? r[project]?.dashboardUrl : Object.values(r).find((x) => x.dashboardUrl)?.dashboardUrl;
};

export function openDashboard(project?: string): void {
  const url = dashboardUrl(project);
  if (url) openInBrowser(url);
  else showDotnetBalloon("The Aspire dashboard is not running", "Run the AppHost (.NET Aspire configuration) first; its output names the dashboard URL.", 6000);
}

async function track(runId: string, project: string): Promise<void> {
  setRunning((r) => ({ ...r, [project]: { started: false } }));
  let notified = false;
  const off = await runApi.onEvent(runId, (e: RunEvent) => {
    if (e.type === "output") {
      void dotnetWebApi.runOutput(e.text).then(({ aspire }) => {
        if (!aspire.dashboardUrl && !aspire.started) return;
        setRunning((r) => ({ ...r, [project]: { dashboardUrl: aspire.dashboardUrl ?? r[project]?.dashboardUrl, started: aspire.started || !!r[project]?.started } }));
        bumpDetails();
        if (aspire.dashboardUrl && !notified) {
          notified = true;
          if (getSetting<boolean>("dotnet.aspire.openDashboard", false)) openInBrowser(aspire.dashboardUrl);
          else showDotnetBalloon("Aspire dashboard", aspire.dashboardUrl.replace(/\?.*$/, ""), 12000, [{ label: "Open Dashboard", run: () => openInBrowser(aspire.dashboardUrl!) }]);
        }
      });
    } else if (e.type === "exited") {
      off();
      setRunning((r) => {
        const next = { ...r };
        delete next[project];
        return next;
      });
      bumpDetails();
    }
  });
}

// ---- Run configuration --------------------------------------------------------------------------

const opt = (cfg: RunConfiguration, key: string) => cfg.options?.[key] ?? "";
let seq = 0;

registerRunConfigurationType({
  id: "dotnet-aspire",
  name: ".NET Aspire",
  description: "Run a .NET Aspire AppHost with dotnet run: its resources show up in the Services tool window, the dashboard link in the console.",
  icon: Boxes,
  color: "var(--syn-type)",
  pluginId: PLUGIN,
  argumentsLabel: "Program arguments",
  noRunTarget: true,
  defaults: () => {
    const h = hosts()?.[0];
    return { options: { project: h?.path ?? "", profile: projectByPath(h?.path ?? "")?.launchProfiles?.[0]?.name ?? "" } };
  },
  fields: [
    { key: "project", label: "AppHost project", kind: "enum", required: true, options: () => (hosts() ?? []).map((h) => ({ value: h.path, label: h.name })) },
    {
      key: "profile",
      label: "Launch profile",
      kind: "enum",
      default: "",
      options: () => {
        const seen = new Map<string, string>();
        for (const h of hosts() ?? [])
          for (const p of projectByPath(h.path)?.launchProfiles ?? [])
            if (p.commandName === "Project" && !seen.has(p.name)) seen.set(p.name, `${p.name}${p.applicationUrl ? ` (${p.applicationUrl.split(";")[0]})` : ""}`);
        return [{ value: "", label: "Default (the first profile)" }, ...[...seen].map(([value, label]) => ({ value, label }))];
      },
    },
    {
      key: "configuration",
      label: "Configuration",
      kind: "enum",
      default: "",
      options: [
        { value: "", label: "Settings | .NET" },
        { value: "Debug", label: "Debug" },
        { value: "Release", label: "Release" },
      ],
    },
    { key: "dotnetArgs", label: "dotnet run options", placeholder: "--property:Foo=Bar" },
  ],
  suggestName: (cfg) => projectByPath(opt(cfg, "project"))?.name,
  start: async (cfg, request) => {
    const runId = request.runId ?? `dotnet-aspire-${++seq}`;
    await track(runId, opt(cfg, "project"));
    return runApi.start({ ...request, runId });
  },
});

/** The AppHost's configuration: a saved one, else a temporary one. */
function configurationFor(host: AppHost): RunConfiguration {
  return (
    allConfigurations().find((c) => c.type === "dotnet-aspire" && c.options?.project === host.path) ??
    useTemporary({ name: host.name, type: "dotnet-aspire", options: { project: host.path } })
  );
}

// ---- Services tool window ---------------------------------------------------------------------------

const ICONS: Record<string, NodeIcon> = { project: "dotnetProject", container: "container", npmapp: "service", executable: "service" };
const iconOf = (r: AspireResource): NodeIcon => ICONS[r.kind] ?? (r.kind === "database" || /sql|postgres|mongo|redis|cosmos|oracle|rabbit|kafka/.test(r.kind) ? "image" : "service");

const isRunningHost = (h: AppHost) => !!runningFor(allConfigurations().find((c) => c.type === "dotnet-aspire" && c.options?.project === h.path)) || !!running()[h.path];

function nodes(): ServiceNode[] {
  if (!aspireActive()) return [];
  return (hosts() ?? []).map((h) => {
    const up = isRunningHost(h);
    const started = !!running()[h.path]?.started;
    return {
      id: `aspire/${h.path}`,
      label: h.name,
      detail: ".NET Aspire",
      icon: "aspire",
      status: up ? (started ? "running" : "busy") : "stopped",
      expanded: true,
      children: h.resources
        .filter((r) => !r.parent)
        .map((r) => ({
          id: `aspire/${h.path}/${r.name}`,
          label: r.name,
          detail: r.kind === "project" ? (projectByPath(r.project ?? "")?.name ?? r.detail ?? "project") : (r.detail ?? r.kind),
          icon: iconOf(r),
          status: started ? "running" : undefined,
          muted: !up,
          children: h.resources
            .filter((c) => c.parent === r.name)
            .map((c) => ({ id: `aspire/${h.path}/${c.name}`, label: c.name, detail: c.kind, icon: iconOf(c), status: started ? "running" : undefined, muted: !up })),
        })),
    } satisfies ServiceNode;
  });
}

function details(nodeId: string): ServiceDetails | null {
  const rest = nodeId.slice("aspire/".length);
  const host = (hosts() ?? []).find((h) => rest === h.path || rest.startsWith(`${h.path}/`));
  if (!host) return null;
  const up = isRunningHost(host);
  const url = running()[host.path]?.dashboardUrl;
  const resource = host.resources.find((r) => rest === `${host.path}/${r.name}`);
  const rows = (resource ? [resource] : host.resources).map((r) => [r.name, r.kind, r.detail ?? "", (r.references ?? []).join(", "), r.external ? "yes" : ""]);
  return {
    nodeId,
    target: { kind: "aspire", name: configurationFor(host).name },
    title: resource ? resource.name : host.name,
    icon: resource ? iconOf(resource) : "aspire",
    status: up ? "running" : "stopped",
    subtitle: resource ? `${resource.kind} of ${host.name}` : host.program,
    tabs: ["dashboard"],
    sections: [
      {
        title: "AppHost",
        rows: [
          { label: "Status", value: up ? (running()[host.path]?.started ? "Running" : "Starting") : "Not started", status: up ? "running" : "stopped" },
          { label: "Dashboard", value: url ? url.replace(/\?.*$/, "") : up ? "Waiting for the dashboard URL…" : "Starts with the AppHost", url: url ?? undefined },
          { label: "Project", value: host.path },
        ],
      },
      { title: "Resources", table: { columns: ["Name", "Type", "Details", "References", "External"], rows } },
    ],
    actions: up ? ["stop", "restart"] : ["start"],
  };
}

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

registerServicesProvider({
  id: "aspire",
  nodes,
  owns: (id) => id.startsWith("aspire/"),
  details: async (id) => details(id),
  refresh: async () => void (await refetch()),
  async action(d, action) {
    const host = (hosts() ?? []).find((h) => d.nodeId.slice(7).startsWith(h.path));
    if (!host) return;
    const cfg = configurationFor(host);
    if (action === "start") await launch(cfg);
    else if (action === "stop") await stopConfiguration(cfg);
    else if (action === "restart") {
      await stopConfiguration(cfg);
      for (let i = 0; i < 50 && runningFor(cfg); i++) await sleep(100);
      await launch(cfg);
    }
    bumpDetails();
  },
});

if (typeof document !== "undefined") createRoot(() => createEffect(on(running, () => bumpDetails(), { defer: true })));
