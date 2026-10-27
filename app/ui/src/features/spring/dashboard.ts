/**
 * The Spring Boot run dashboard in the Services tool window (IntelliJ IDEA's "Services" for
 * Spring Boot): Spring Boot run configurations, grouped into Running and Not Started, with
 * port, profiles and health; the details show the actuator's health, request mappings and
 * beans. Nodes and details are computed in Rust (`spring_dashboard*`), which follows runs
 * through their console output and asks the actuator on localhost. Start, Stop and Restart
 * go through the run configurations, so Safe Mode, Before launch tasks and the Run tool
 * window apply.
 */
import { createEffect, createRoot, createSignal, on, onCleanup } from "solid-js";
import { springApi, SPRING_BOOT } from "@/ipc/spring";
import type { ServiceNode } from "@/ipc/services";
import { registerServicesProvider } from "@/features/services/providers";
import { bumpDetails } from "@/features/services/store";
import { configurations, findConfiguration, temporary } from "@/features/run/configStore";
import { launch, runningFor, stopConfiguration } from "@/features/run/runStore";
import { getSetting } from "@/state/settings";
import { workspace } from "@/state/workspace";
import { springActive } from "./store";

const [nodes, setNodes] = createSignal<ServiceNode[]>([]);

const hasBootConfigurations = () => [...configurations(), ...temporary()].some((c) => c.type === SPRING_BOOT);

let seq = 0;
async function reload(): Promise<void> {
  const id = ++seq;
  if (!workspace() || (!springActive() && !hasBootConfigurations())) {
    setNodes([]);
    return;
  }
  const list = await springApi.dashboard().catch(() => [] as ServiceNode[]);
  if (id === seq) setNodes(list);
  bumpDetails();
}

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

registerServicesProvider({
  id: "spring",
  nodes,
  owns: (id) => id === "spring" || id.startsWith("spring/"),
  details: (id) => springApi.dashboardDetails(id),
  refresh: reload,
  async action(d, action) {
    const name = d.target.name;
    const cfg = name ? findConfiguration(name) : undefined;
    if (!cfg) throw new Error(`No run configuration '${name ?? d.title}'`);
    if (action === "start") await launch(cfg);
    else if (action === "stop") await stopConfiguration(cfg);
    else if (action === "restart") {
      await stopConfiguration(cfg);
      for (let i = 0; i < 50 && runningFor(cfg); i++) await sleep(100);
      await launch(cfg);
    }
    void reload();
  },
});

if (typeof document !== "undefined") {
  createRoot(() => {
    void springApi.onDashboard(() => void reload());
    createEffect(on([() => workspace()?.root, springActive, configurations, temporary], () => void reload()));
    // While an application runs, refresh its health (the details ask the actuator again).
    createEffect(() => {
      const running = JSON.stringify(nodes()).includes('"status":"running"') || JSON.stringify(nodes()).includes('"status":"busy"');
      if (!running) return;
      const seconds = Math.max(2, getSetting<number>("spring.dashboard.refreshSeconds", 5) ?? 5);
      const timer = setInterval(() => void reload(), seconds * 1000);
      onCleanup(() => clearInterval(timer));
    });
  });
}
