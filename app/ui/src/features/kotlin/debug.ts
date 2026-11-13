/**
 * The Kotlin debugger. Debug (Shift+F9) on a Kotlin configuration (a Kotlin main, a Kotest
 * spec, JUnit tests in Kotlin, Remote JVM Debug in a Kotlin project) asks the backend for a
 * plan (`kotlin_debug_prepare`): kotlin-lsp's own DAP server (`start_debug_server`), which
 * maps Kotlin lines and binds breakpoints in `.kt` files, or java-debug when chosen in the
 * settings. The run starts with a JDWP agent that waits for the debugger
 * (`FORGE_JAVA_DEBUG_PORT`) and the session attaches once the JVM listens. The Debug tool
 * window gets a Coroutines tab (`CoroutinesView.tsx`).
 */
import { listen } from "@/ipc";
import { DEBUG_PORT_ENV } from "@/ipc/java";
import { kotlinApi, type KotlinDebugPlan } from "@/ipc/kotlin";
import type { RunConfiguration, RunEvent } from "@/ipc/run";
import { dapApi, registerDebugAdapter, registerDebugTab } from "@/features/debug";
import { refreshDebugAdapters } from "@/features/debug/adapters";
import { registerDebugLauncher } from "@/features/run";
import { balloonsFor } from "@/keymap/dispatch";
import { showToolWindow } from "@/state/layout";
import { CoroutinesView } from "./CoroutinesView";
import { isKotlinConfiguration } from "./runTypes";
import { active } from "./store";
import { t } from "@/i18n";

const showBalloon = balloonsFor("Kotlin Debugger");

registerDebugAdapter({ type: "kotlin", label: "Kotlin (kotlin-lsp)", languages: ["kt", "kts"] });

const LISTENING = /Listening for transport dt_socket at address: ?(\d+)/;
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

/** Attach the planned debugger to the JVM's JDWP agent. */
export function attach(cfg: RunConfiguration, plan: KotlinDebugPlan) {
  return dapApi.start({
    adapter: plan.adapter,
    mode: "attachServer",
    host: "127.0.0.1",
    port: plan.dapPort,
    name: cfg.name,
    configuration:
      plan.adapter === "kotlin"
        ? { type: "intellij_jvm", request: "attach", hostName: plan.hostName, port: plan.jdwpPort }
        : { hostName: plan.hostName, port: plan.jdwpPort, timeout: 30000 },
  });
}

async function attachWhenListening(cfg: RunConfiguration, plan: KotlinDebugPlan, runId: string, exited: Promise<unknown>): Promise<void> {
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
  // Gradle and kotlinc compile first; a JVM that never says it listens is tried anyway.
  await Promise.race([listening, sleep(120000)]);
  unlisten();
  for (let attempt = 0; !over && attempt < 10; attempt++) {
    try {
      await attach(cfg, plan);
      return;
    } catch (e) {
      if (attempt === 9) showBalloon(t("langs.kotlin.cannotAttachTheDebuggerTo", { name: cfg.name }), String(e).replace(/^Error: /, ""), 8000);
      await sleep(1000);
    }
  }
}

registerDebugLauncher({
  id: "kotlin",
  canDebug: (cfg) => isKotlinConfiguration(cfg) || (cfg.type === "jvm-remote-debug" && active()),
  async launch(cfg, ctx) {
    await refreshDebugAdapters();
    let plan: KotlinDebugPlan;
    try {
      plan = await kotlinApi.debugPrepare(cfg);
    } catch (e) {
      showBalloon(t("common.cannotDebug", { name: cfg.name }), String(e).replace(/^Error: /, ""), 10000);
      return;
    }
    showToolWindow("debug");
    if (!plan.run) {
      try {
        await attach(cfg, plan);
      } catch (e) {
        showBalloon(t("langs.kotlin.cannotAttachTo", { hostName: plan.hostName, jdwpPort: plan.jdwpPort }), String(e).replace(/^Error: /, ""), 8000);
      }
      return;
    }
    const handle = await ctx.run({ env: { [DEBUG_PORT_ENV]: String(plan.jdwpPort) } });
    if (!handle) return;
    await attachWhenListening(cfg, plan, handle.started.runId, handle.exited);
  },
});

registerDebugTab({
  id: "coroutines",
  title: "Coroutines",
  when: (s) => active() && (s?.adapter === "kotlin" || s?.adapter === "java"),
  component: CoroutinesView,
});
