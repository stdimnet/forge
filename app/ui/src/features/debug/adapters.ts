/**
 * Debug adapters: the contribution point packs use to plug a debugger into the shared
 * Debug tool window.
 *
 * Adapters are declared in a plugin manifest (`[[contributions.debugAdapters]]`: how to
 * start the adapter and the launch/attach templates, see `forge_dap::ManifestAdapter`) or in
 * Rust (`forge_dap::DebugAdapterDescriptor`). A pack's UI adds what only it knows with
 * `registerDebugAdapter`: which run configuration types the adapter debugs and how a
 * configuration becomes a launch request (`resolve`).
 */
import { createRoot, createSignal } from "solid-js";
import type { RunConfiguration } from "@/ipc/run";
import { dapApi, type AdapterInfo, type LaunchRequest } from "./dapApi";

export interface DebugAdapterRegistration {
  /** Adapter type, as in the manifest (`lldb`, `debugpy`). */
  type: string;
  /** Shown in dialogs; defaults to the manifest's label. */
  label?: string;
  /** Run configuration types that Debug (Shift+F9) starts with this adapter. */
  runConfigurationTypes?: string[];
  /** File extensions with line breakpoints (added to the manifest's `languages`). */
  languages?: string[];
  /** JSON schema of extra launch options (forms, validation). */
  launchSchema?: unknown;
  attachSchema?: unknown;
  /** Build the launch request of a run configuration (default: `defaultLaunchRequest`). */
  resolve?: (cfg: RunConfiguration) => LaunchRequest | Promise<LaunchRequest>;
}

/** An adapter as the UI uses it: the backend's info merged with the UI registration. */
export type DebugAdapter = AdapterInfo & Omit<DebugAdapterRegistration, "label" | "languages">;

const state = createRoot(() => {
  const [registrations, setRegistrations] = createSignal<DebugAdapterRegistration[]>([]);
  const [backend, setBackend] = createSignal<AdapterInfo[]>([]);
  return { registrations, setRegistrations, backend, setBackend };
});

/** Register (or extend) a debug adapter. Returns a function that removes the registration. */
export function registerDebugAdapter(adapter: DebugAdapterRegistration): () => void {
  state.setRegistrations((prev) => [...prev.filter((a) => a.type !== adapter.type), adapter]);
  return () => state.setRegistrations((prev) => prev.filter((a) => a !== adapter));
}

/** Re-read the adapters of the enabled plugins (after enabling plugins or changing paths). */
export async function refreshDebugAdapters(): Promise<void> {
  try {
    state.setBackend(await dapApi.adapters());
  } catch (e) {
    console.warn("debug adapters unavailable", e);
  }
}

/** All adapters: those the backend can start, extended by UI registrations. */
export function debugAdapters(): DebugAdapter[] {
  const regs = state.registrations();
  return state.backend().map((info) => {
    const reg = regs.find((r) => r.type === info.type);
    return {
      ...info,
      ...reg,
      label: reg?.label ?? info.label,
      languages: [...new Set([...info.languages, ...(reg?.languages ?? [])])],
    };
  });
}

export const findDebugAdapter = (type: string | undefined) => (type ? debugAdapters().find((a) => a.type === type) : undefined);

/** The adapter that debugs run configurations of `type`. */
export const adapterForRunType = (type: string) => debugAdapters().find((a) => a.runConfigurationTypes?.includes(type));

const extension = (path: string) => /\.([^./\\]+)$/.exec(path)?.[1]?.toLowerCase() ?? "";

/** Whether line breakpoints can be set in `path`: PHP files and files of any adapter's languages. */
export function isDebuggableFile(path: string): boolean {
  const ext = extension(path);
  if (["php", "phtml", "inc"].includes(ext)) return true;
  return !!ext && debugAdapters().some((a) => a.languages.includes(ext));
}

/** A launch request from the common run configuration fields (program, arguments, …). */
export function defaultLaunchRequest(adapter: string, cfg: RunConfiguration): LaunchRequest {
  const o = cfg.options ?? {};
  let configuration: Record<string, unknown> | undefined;
  if (o.launchOptions?.trim()) {
    try {
      configuration = JSON.parse(o.launchOptions) as Record<string, unknown>;
    } catch {
      throw new Error("Launch options are not valid JSON");
    }
  }
  return {
    mode: "launch",
    adapter,
    name: cfg.name,
    program: o.program,
    arguments: cfg.arguments,
    cwd: cfg.workingDirectory || undefined,
    env: cfg.env,
    stopOnEntry: o.stopOnEntry === "true",
    configuration,
  };
}
