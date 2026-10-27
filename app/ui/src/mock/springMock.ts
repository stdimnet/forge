/**
 * Browser-mode Spring Boot and Jakarta EE: the `spring_*` commands on the demo project
 * (`?project=spring`, demoSpringProject.ts). The model, gutter markers, configuration file
 * assistance, HTTP Client requests, run configurations and the run dashboard go through the
 * Rust `javaforge-fw-spring` crate (WebAssembly), like in the desktop app; the metadata of
 * the "jars", the application's console and its actuator answers are canned. Spring Boot runs
 * play a startup log and keep running until stopped. Nothing is run.
 */
import { emitMock, listen, mockHandler, registerMock } from "@/ipc";
import type { CommandLine, RunConfiguration, RunConfigurations, RunEvent, RunRequest } from "@/ipc/run";
import type { EndpointRow } from "@/ipc/spring";
import { callWasmSync, forgeWasm } from "@/wasm";
import { isSpringDemo, springDemoActuator, springDemoConsole, springDemoEnums, springDemoMetadata } from "./demoSpringProject";
import { mockFiles } from "./workspaceMock";
import { startMockRun, type Step } from "./runMock";

const SPRING_BOOT = "spring-boot";
const DEBUG_ENV = "FORGE_JAVA_DEBUG_PORT";
const DASHBOARD = "spring://dashboard";

interface Observed {
  port?: number;
  contextPath?: string;
  server?: string;
  profiles?: string[];
  pid?: number;
  started: boolean;
  startupSeconds?: number;
  failed: boolean;
  bootVersion?: string;
}

interface AppRun {
  runId: string;
  mainClass: string;
  profiles: string[];
  running: boolean;
  debug: boolean;
  exitCode?: number | null;
  lines: string[];
  observed: Observed;
}

/** Spring Boot runs by configuration name. */
const apps = new Map<string, AppRun>();

const relevant = (path: string) =>
  /\.(java|kt)$/.test(path) || /(^|\/)(application|bootstrap)(-[\w.-]+)?\.(properties|ya?ml)$/.test(path) || /META-INF\/.*spring-configuration-metadata\.json$/.test(path);

function projectFiles(): [string, string][] {
  return Object.entries(mockFiles).filter(([p]) => relevant(p));
}

/** The running application whose actuator answers (the demo's). */
function running(): [string, AppRun] | undefined {
  return [...apps.entries()].find(([, a]) => a.running && a.observed.started);
}

function runtime() {
  const r = running();
  if (!r) return undefined;
  const port = r[1].observed.port ?? 8080;
  return { baseUrl: `http://localhost:${port}/actuator`, ...springDemoActuator };
}

async function request(extra: Record<string, unknown> = {}) {
  await forgeWasm();
  return { files: projectFiles(), metadata: springDemoMetadata, enums: springDemoEnums, ...extra };
}

registerMock("spring_model", async () => {
  const wasm = await forgeWasm();
  const out = callWasmSync<Record<string, unknown>>(wasm.spring_overview, await request({ runtime: runtime() }));
  const r = running();
  if (r) out.runtime = { name: r[0], url: `http://localhost:${r[1].observed.port ?? 8080}${r[1].observed.contextPath ?? ""}`, beans: true, mappings: true };
  return out;
});

registerMock("spring_gutter", async ({ path, content }) => callWasmSync((await forgeWasm()).spring_gutter, await request({ path, content })));

for (const kind of ["complete", "inspect", "hover", "goto"]) {
  registerMock(`spring_config_${kind}`, async ({ path, content, offset }) => {
    const wasm = await forgeWasm();
    return JSON.parse(wasm.spring_config(kind, JSON.stringify(await request({ path, content, offset: offset ?? 0 }))));
  });
}

registerMock("spring_http_request", async ({ endpoint }) => {
  const r = running();
  const baseUrl = r ? `http://localhost:${r[1].observed.port ?? 8080}${r[1].observed.contextPath ?? ""}` : "http://localhost:8080";
  return callWasmSync((await forgeWasm()).spring_http_request, { endpoint: endpoint as EndpointRow, baseUrl });
});

// ---- Run dashboard --------------------------------------------------------------------------

async function savedBootConfigurations(): Promise<RunConfiguration[]> {
  const load = mockHandler("run_configurations_load");
  const all = (load ? ((await load({})) as RunConfigurations) : { configurations: [] }).configurations ?? [];
  return all.filter((c) => c.type === SPRING_BOOT);
}

const profilesOf = (cfg: RunConfiguration) =>
  (cfg.options?.activeProfiles ?? "")
    .split(/[,\s]+/)
    .map((p) => p.trim())
    .filter(Boolean);

async function dashboardApps() {
  const saved = await savedBootConfigurations();
  const out = saved.map((c) => {
    const a = apps.get(c.name);
    return {
      name: c.name,
      mainClass: c.options?.mainClass ?? "",
      profiles: profilesOf(c),
      runId: a?.runId,
      running: !!a?.running,
      debug: !!a?.debug,
      exitCode: a?.exitCode ?? undefined,
      observed: a?.observed ?? { started: false, failed: false },
      configuredPort: 8080,
      health: a?.running && a.observed.started ? "UP" : undefined,
    };
  });
  for (const [name, a] of apps) {
    if (out.some((x) => x.name === name)) continue;
    out.push({ name, mainClass: a.mainClass, profiles: a.profiles, runId: a.runId, running: a.running, debug: a.debug, exitCode: a.exitCode ?? undefined, observed: a.observed, configuredPort: 8080, health: a.running ? "UP" : undefined });
  }
  return out;
}

registerMock("spring_dashboard", async () => callWasmSync((await forgeWasm()).spring_dashboard, { apps: await dashboardApps() }));
registerMock("spring_dashboard_details", async ({ nodeId }) => {
  const wasm = await forgeWasm();
  return callWasmSync(wasm.spring_dashboard, { apps: await dashboardApps(), nodeId, runtime: runtime() });
});

// ---- Spring Boot runs ---------------------------------------------------------------------

const prevStart = mockHandler("run_start")!;
registerMock("run_start", async (args) => {
  const r = args.request as RunRequest;
  const cfg = r.command ? undefined : r.configuration;
  if (!cfg || cfg.type !== SPRING_BOOT) return prevStart(args);
  const wasm = await forgeWasm();
  const launch = callWasmSync<{ vmOptions: string; arguments: string }>(wasm.spring_launch, {
    options: cfg.options ?? {},
    vmOptions: cfg.interpreterOptions ?? "",
    arguments: cfg.arguments ?? "",
  });
  // The command line of the Application run it becomes (JavaForge's mock builds it).
  const app: RunConfiguration = { ...cfg, type: "java-application", interpreterOptions: launch.vmOptions, arguments: launch.arguments };
  const built = (await mockHandler("run_command_line")!({ request: { ...r, configuration: app } })) as CommandLine;
  const runId = r.runId ?? `mock-spring-run-${Date.now()}`;
  const debugPort = r.env?.[DEBUG_ENV] ? Number(r.env[DEBUG_ENV]) : null;
  const profiles = profilesOf(cfg);
  const pid = 48211;
  apps.set(cfg.name, { runId, mainClass: cfg.options?.mainClass ?? "", profiles, running: true, debug: !!debugPort, lines: [], observed: { started: false, failed: false } });
  emitMock(DASHBOARD, null);
  const unlisten = await listen<RunEvent>(`run://${runId}`, (e) => {
    const a = apps.get(cfg.name);
    if (!a || a.runId !== runId) return;
    if (e.type === "output") {
      a.lines.push(...e.text.split("\n"));
      const before = JSON.stringify(a.observed);
      a.observed = callWasmSync<Observed>(wasm.spring_observe, a.lines);
      if (JSON.stringify(a.observed) !== before) emitMock(DASHBOARD, null);
    } else if (e.type === "exited") {
      a.running = false;
      a.exitCode = e.code;
      unlisten();
      emitMock(DASHBOARD, null);
    }
  });
  const steps: Step[] = [];
  if (debugPort) steps.push([150, `Listening for transport dt_socket at address: ${debugPort}\n`], [2500, ""]);
  const lines = springDemoConsole(profiles, Number(/--server\.port=(\d+)/.exec(launch.arguments)?.[1] ?? 8080), pid);
  lines.forEach((l, i) => steps.push([i < 10 ? 25 : 260, `${l}\n`]));
  return startMockRun(runId, built, steps);
});

// The demo's Spring Boot configuration, next to JavaForge's (javaMock.ts saves those first).
if (isSpringDemo()) {
  void Promise.resolve()
    .then(() => Promise.resolve())
    .then(async () => {
      const load = mockHandler("run_configurations_load");
      const save = mockHandler("run_configurations_save");
      if (!load || !save) return;
      const current = (await load({})) as RunConfigurations;
      const boot: RunConfiguration = {
        name: "InventoryApplication",
        type: SPRING_BOOT,
        storeAsProjectFile: true,
        options: { mainClass: "com.acme.inventory.InventoryApplication", activeProfiles: "dev", exposeActuator: "true" },
      };
      const prod: RunConfiguration = {
        name: "InventoryApplication (prod)",
        type: SPRING_BOOT,
        storeAsProjectFile: true,
        options: { mainClass: "com.acme.inventory.InventoryApplication", activeProfiles: "prod", overrideProperties: "server.port=9090", exposeActuator: "true" },
      };
      await save({
        configurations: {
          ...current,
          selected: boot.name,
          configurations: [boot, prod, ...(current.configurations ?? []).filter((c) => c.type !== SPRING_BOOT)],
        },
      });
    });
}
