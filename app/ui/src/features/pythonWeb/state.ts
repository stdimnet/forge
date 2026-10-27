/**
 * PyForge web frameworks in the UI: the analysed project (`python_web_project`, computed in
 * Rust by `packs/pyforge/frameworks`), whether the Django / Flask / FastAPI / Celery plugins
 * apply, and the actions the tool windows share (open a location, Open in HTTP Client,
 * open the served page).
 */
import { createSignal } from "solid-js";
import { pythonWebApi, type WebLocation, type WebProject, type WebRoute } from "@/ipc/pythonWeb";
import { scratchesApi } from "@/ipc/scratches";
import type { RunConfiguration } from "@/ipc/run";
import { revealLocation } from "@/features/lsp/navigation";
import { openInBrowser } from "@/features/run/runStore";
import { configurations } from "@/features/run/configStore";
import { openScratch, refreshScratches } from "@/features/scratches/store";
import { showBalloon } from "@/keymap";
import { isPluginActive } from "@/state/project";

export const webActive = () => isPluginActive("forge.python-web");
export const djangoActive = () => isPluginActive("forge.django");
export const flaskActive = () => isPluginActive("forge.flask");
export const fastapiActive = () => isPluginActive("forge.fastapi");
export const celeryActive = () => isPluginActive("forge.celery");
/** Any of the plugins (the editor features and the Endpoints window apply). */
export const anyWebActive = () => webActive() || celeryActive();

const [project, setProject] = createSignal<WebProject | null>(null);
const [loading, setLoading] = createSignal(false);
const [error, setError] = createSignal<string | null>(null);

export { project as webProject, loading as webLoading, error as webError };

export async function loadWebProject(refresh = false): Promise<void> {
  setLoading(true);
  try {
    setProject(await pythonWebApi.project(refresh));
    setError(null);
  } catch (e) {
    setError(String(e).replace(/^Error: /, ""));
  } finally {
    setLoading(false);
  }
}

export const reveal = (l: WebLocation) => revealLocation(l.path, { line: l.line, character: l.character });

/** Display order of HTTP methods. */
const ORDER = ["GET", "POST", "PUT", "PATCH", "DELETE", "OPTIONS", "HEAD", "TRACE", "WS"];

export function sortedMethods(r: WebRoute): string[] {
  if (!r.methods.length) return ["ANY"];
  return [...r.methods].sort((a, b) => ORDER.indexOf(a) - ORDER.indexOf(b));
}

const DEFAULT_PORTS: Record<WebRoute["framework"], number> = { django: 8000, flask: 5000, fastapi: 8000 };
const RUN_TYPES: Record<WebRoute["framework"], string> = { django: "django-server", flask: "flask-server", fastapi: "fastapi" };

/** The server a framework's routes are served by: the first run configuration's host:port. */
export function serverBase(framework: WebRoute["framework"], list: RunConfiguration[] = configurations()): string {
  const cfg = list.find((c) => c.type === RUN_TYPES[framework]);
  const host = cfg?.options?.host?.trim() || "127.0.0.1";
  const port = cfg?.options?.port?.trim() || String(DEFAULT_PORTS[framework]);
  return `http://${host === "0.0.0.0" ? "127.0.0.1" : host}:${port}`;
}

/** The request of an endpoint in the HTTP client's format. */
export function httpRequest(r: WebRoute, base = serverBase(r.framework)): string {
  const method = sortedMethods(r).find((m) => m !== "ANY" && m !== "WS") ?? "GET";
  const lines = [`### ${r.name ?? r.handler}`, `${method} ${base}${r.samplePath}`];
  if (["POST", "PUT", "PATCH"].includes(method)) lines.push("Content-Type: application/json", "", "{}");
  return `${lines.join("\n")}\n`;
}

/** "Open in HTTP Client": a new HTTP request scratch with the endpoint's request. */
export async function openInHttpClient(r: WebRoute): Promise<void> {
  try {
    const file = await scratchesApi.create("http", httpRequest(r));
    await refreshScratches();
    await openScratch(file);
  } catch (e) {
    showBalloon("Cannot open the HTTP client", String(e));
  }
}

export function openRouteInBrowser(r: WebRoute): void {
  openInBrowser(`${serverBase(r.framework)}${r.samplePath}`);
}

/** FastAPI's interactive API documentation (Swagger UI). */
export function openFastApiDocs(): void {
  openInBrowser(`${serverBase("fastapi")}/docs`);
}
