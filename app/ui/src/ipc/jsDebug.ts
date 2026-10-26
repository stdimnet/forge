/**
 * The JavaScript debugger (`app/src-tauri/src/commands/js_debug.rs`, logic in
 * `packs/webforge/js-debug`): js-debug's status and consented download, the browsers for
 * "JavaScript Debug", and the launch request of a run configuration (computed in Rust).
 */
import type { LaunchRequest } from "@/features/debug/dapApi";
import { invoke, listen } from "./index";
import type { RunConfiguration } from "./run";

/** The js-debug release Forge downloads (pinned, SHA-256 checked). */
export interface JsDebugRelease {
  version: string;
  url: string;
  sha256: string;
  size: number;
}

export interface JsDebugStatus {
  /** The DAP server script (`dapDebugServer.js`) Forge uses; absent until installed. */
  server?: string | null;
  /** `server` is the user's own folder (Settings | Node.js | js-debug folder). */
  custom: boolean;
  release: JsDebugRelease;
  /** The Node.js that runs js-debug (empty: none found; the download brings one). */
  node: string;
}

export interface Browser {
  kind: "chrome" | "edge" | "chromium";
  name: string;
  path: string;
}

/** `js-debug://install` while downloading: a step or download progress. */
export type JsDebugInstallEvent = { message: string } | { label: string; done: number; total?: number | null };

export const jsDebugApi = {
  status: () => invoke<JsDebugStatus>("js_debug_status"),
  /** Download js-debug (and Node.js when there is none); call only after the user agreed. */
  install: () => invoke<JsDebugStatus>("js_debug_install"),
  browsers: () => invoke<Browser[]>("js_debug_browsers"),
  request: (configuration: RunConfiguration) => invoke<LaunchRequest>("js_debug_request", { configuration }),
  onInstall: (cb: (e: JsDebugInstallEvent) => void) => listen<JsDebugInstallEvent>("js-debug://install", cb),
};
