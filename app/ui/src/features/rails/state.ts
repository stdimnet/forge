/**
 * Ruby on Rails in the UI: whether the plugin applies (`forge.rails`: a Rails application),
 * the analysed project (`rails_project`, computed in Rust by `packs/rubyforge/fw-rails`),
 * and the actions the views share (open a location, Open in HTTP Client, open the served
 * page, run commands in the Run tool window or a Terminal tab).
 */
import { createSignal } from "solid-js";
import { railsApi, type RailsLocation, type RailsProject, type RailsRoute } from "@/ipc/rails";
import { scratchesApi } from "@/ipc/scratches";
import type { RunConfiguration } from "@/ipc/run";
import { revealLocation } from "@/features/lsp/navigation";
import { openInBrowser } from "@/features/run/runStore";
import { configurations } from "@/features/run/configStore";
import { openScratch, refreshScratches } from "@/features/scratches/store";
import { balloonsFor } from "@/keymap/dispatch";
import { isPluginActive } from "@/state/project";

export const PLUGIN = "forge.rails";
export const railsActive = () => isPluginActive(PLUGIN);
export const showRailsBalloon = balloonsFor("Rails");
export const errorText = (e: unknown) => String(e).replace(/^Error: /, "");

const [project, setProject] = createSignal<RailsProject | null>(null);
const [loading, setLoading] = createSignal(false);
const [error, setError] = createSignal<string | null>(null);

export { project as railsProject, loading as railsLoading, error as railsError };

export async function loadRailsProject(refresh = false): Promise<RailsProject | null> {
  setLoading(true);
  try {
    const p = await railsApi.project(refresh);
    setProject(p);
    setError(null);
    return p;
  } catch (e) {
    setError(errorText(e));
    return null;
  } finally {
    setLoading(false);
  }
}

export const reveal = (l: RailsLocation) => revealLocation(l.path, { line: l.line, character: l.character });

/** Display order of HTTP methods. */
const ORDER = ["GET", "POST", "PUT", "PATCH", "DELETE", "OPTIONS", "HEAD"];

export function sortedMethods(r: RailsRoute): string[] {
  if (!r.methods.length) return ["ANY"];
  return [...r.methods].sort((a, b) => ORDER.indexOf(a) - ORDER.indexOf(b));
}

/** `articles#index`, `Sidekiq::Web`, `redirect(/articles)`. */
export function handlerOf(r: RailsRoute): string {
  if (r.kind === "mount") return r.controller;
  if (r.kind === "redirect") return `redirect(${r.action})`;
  return `${r.controller}#${r.action}`;
}

/** The server routes are served by: the first "Rails" configuration's host and port. */
export function serverBase(list: RunConfiguration[] = configurations()): string {
  const cfg = list.find((c) => c.type === "rails");
  const host = cfg?.options?.host?.trim() || "localhost";
  const port = cfg?.options?.port?.trim() || "3000";
  return `http://${host === "0.0.0.0" ? "localhost" : host}:${port}`;
}

/** The request of a route in the HTTP client's format. */
export function httpRequest(r: RailsRoute, base = serverBase()): string {
  const method = sortedMethods(r).find((m) => m !== "ANY") ?? "GET";
  const lines = [`### ${r.name ? `${r.name} (${handlerOf(r)})` : handlerOf(r)}`, `${method} ${base}${r.samplePath}`];
  if (["POST", "PUT", "PATCH"].includes(method)) lines.push("Content-Type: application/json", "Accept: application/json", "", "{}");
  else lines.push("Accept: text/html");
  return `${lines.join("\n")}\n`;
}

/** Generate HTTP Request: a new HTTP request scratch with the route's request. */
export async function openInHttpClient(r: RailsRoute): Promise<void> {
  try {
    const file = await scratchesApi.create("http", httpRequest(r));
    await refreshScratches();
    await openScratch(file);
  } catch (e) {
    showRailsBalloon("Cannot open the HTTP client", errorText(e));
  }
}

export function openRouteInBrowser(r: RailsRoute): void {
  openInBrowser(`${serverBase()}${r.samplePath}`);
}
