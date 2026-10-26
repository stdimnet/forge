/**
 * The Java debugger: Microsoft's java-debug serves DAP from inside Eclipse JDT LS. Debug
 * (Shift+F9) on a Java configuration asks the backend for java-debug's DAP port and a JDWP
 * port (`java_debug_prepare`), starts the run with a JDWP agent that waits for the debugger
 * (`FORGE_JAVA_DEBUG_PORT`; Gradle `--debug-jvm`, Surefire `-Dmaven.surefire.debug`), and
 * attaches once the JVM listens. Remote JVM Debug attaches without a run. Hot code replace
 * follows Settings | Languages & Frameworks | Java (java-debug reloads changed classes
 * after JDT LS rebuilt them).
 */
import { listen } from "@/ipc";
import { DEBUG_PORT_ENV, javaApi, type DebugPlan } from "@/ipc/java";
import type { RunConfiguration, RunEvent } from "@/ipc/run";
import { dapApi, registerDebugAdapter } from "@/features/debug";
import { refreshDebugAdapters } from "@/features/debug/adapters";
import { registerDebugLauncher } from "@/features/run";
import { balloonsFor } from "@/keymap/dispatch";
import { showToolWindow } from "@/state/layout";
import { DEBUG_TYPES } from "./runTypes";

const showBalloon = balloonsFor("Java Debugger");

registerDebugAdapter({ type: "java", label: "Java (java-debug)", languages: ["java"] });

const LISTENING = /Listening for transport dt_socket at address: ?(\d+)/;
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

/** Attach java-debug to the JVM's JDWP agent. */
export function attach(cfg: RunConfiguration, plan: DebugPlan) {
  return dapApi.start({
    adapter: "java",
    mode: "attachServer",
    host: "127.0.0.1",
    port: plan.dapPort,
    name: cfg.name,
    configuration: { hostName: plan.hostName, port: plan.jdwpPort, timeout: 30000 },
  });
}

/** Attach when the run's output says the JDWP agent listens (or after a while, retrying). */
async function attachWhenListening(cfg: RunConfiguration, plan: DebugPlan, runId: string, exited: Promise<unknown>): Promise<void> {
  let ready: () => void = () => {};
  const listening = new Promise<void>((r) => (ready = r));
  let over = false;
  void exited.then(() => {
    over = true;
    ready();
  });
  const unlisten = await listen<RunEvent>(`run://${runId}`, (e) => {
    if (e.type === "output" && LISTENING.test(e.text)) ready();
  });
  // Gradle compiles first; a JVM that never says it listens is tried anyway.
  await Promise.race([listening, sleep(cfg.type === "java-application" ? 4000 : 120000)]);
  unlisten();
  for (let attempt = 0; !over && attempt < 10; attempt++) {
    try {
      await attach(cfg, plan);
      return;
    } catch (e) {
      if (attempt === 9) showBalloon(`Cannot attach the debugger to '${cfg.name}'`, String(e).replace(/^Error: /, ""), 8000);
      await sleep(1000);
    }
  }
}

registerDebugLauncher({
  id: "java",
  canDebug: (cfg) => DEBUG_TYPES.has(cfg.type),
  async launch(cfg, ctx) {
    await refreshDebugAdapters();
    let plan: DebugPlan;
    try {
      plan = await javaApi.debugPrepare(cfg);
    } catch (e) {
      showBalloon(`Cannot debug '${cfg.name}'`, String(e).replace(/^Error: /, ""), 10000);
      return;
    }
    showToolWindow("debug");
    if (!plan.run) {
      try {
        await attach(cfg, plan);
      } catch (e) {
        showBalloon(`Cannot attach to ${plan.hostName}:${plan.jdwpPort}`, String(e).replace(/^Error: /, ""), 8000);
      }
      return;
    }
    const handle = await ctx.run({ env: { [DEBUG_PORT_ENV]: String(plan.jdwpPort) } });
    if (!handle) return;
    await attachWhenListening(cfg, plan, handle.started.runId, handle.exited);
  },
});
