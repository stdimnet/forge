/** Symfony project data for the tool window, status bar and popups. */
import { createEffect, createRoot, createSignal, on } from "solid-js";
import {
  symfonyApi,
  type ConsoleCommand,
  type SymfonyInfo,
  type SymfonyParameter,
  type SymfonyRoute,
  type SymfonyService,
} from "@/ipc/symfony";
import { isPluginActive, usesFramework } from "@/state/project";
import { workspace } from "@/state/workspace";

/** The Symfony plugin applies to the open project. */
export const symfonyActive = (): boolean => usesFramework("symfony") && isPluginActive("forge.symfony");

const [info, setInfo] = createSignal<SymfonyInfo | null>(null);
const [routes, setRoutes] = createSignal<SymfonyRoute[]>([]);
const [services, setServices] = createSignal<SymfonyService[]>([]);
const [parameters, setParameters] = createSignal<SymfonyParameter[]>([]);
const [commands, setCommands] = createSignal<ConsoleCommand[]>([]);
const [loading, setLoading] = createSignal(false);

export { info as symfonyInfo, routes, services, parameters, commands, loading };

let pending: Promise<void> | null = null;

/** Load (or with `force`, rescan) everything the Symfony views show. */
export function loadSymfony(force = false): Promise<void> {
  if (pending && !force) return pending;
  setLoading(true);
  pending = (async () => {
    try {
      const i = force ? await symfonyApi.refresh() : await symfonyApi.detect();
      setInfo(i);
      if (!i) return;
      const [r, s, p, c] = await Promise.all([
        symfonyApi.routes(),
        symfonyApi.services(),
        symfonyApi.parameters(),
        symfonyApi.consoleCommands(),
      ]);
      setRoutes(r);
      setServices(s);
      setParameters(p);
      setCommands(c);
    } catch (e) {
      console.warn("Symfony index failed", e);
    } finally {
      setLoading(false);
      pending = null;
    }
  })();
  return pending;
}

/** `7.2.3` -> `7.2`. */
export const shortVersion = (v?: string) => (v ? v.split(".").slice(0, 2).join(".") : "");

if (typeof window !== "undefined") {
  createRoot(() =>
    createEffect(
      on(
        () => [workspace()?.root, symfonyActive()] as const,
        ([root, active]) => {
          if (root && active) void loadSymfony();
          else setInfo(null);
        },
      ),
    ),
  );
}
