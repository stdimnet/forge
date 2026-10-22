/**
 * Run configuration types of the PHP debugger: "PHP Web Page" (opens a page of a server
 * from Settings | PHP | Servers; Debug adds `XDEBUG_SESSION_START`) and "PHP HTTP Request"
 * (sends a request; Debug adds the `XDEBUG_SESSION` cookie). Both start no process: their
 * runs are reported on `run://<runId>` by the backend.
 */
import { Globe, Send } from "lucide-solid";
import { registerRunConfigurationType } from "@/features/run/types";
import type { RunConfiguration, RunRequest } from "@/ipc/run";
import { scratchesApi } from "@/ipc/scratches";
import { debugApi } from "./api";
import { knownServers } from "./store";

const opt = (cfg: RunConfiguration, key: string) => cfg.options?.[key] ?? "";
/** Debug runs carry the Xdebug environment of the debug launcher. */
const isDebug = (request: RunRequest) => !!request.env?.XDEBUG_SESSION;

const serverOptions = (allowNone: boolean) => () => [
  ...(allowNone ? [{ value: "", label: "<no server: absolute URL>" }] : []),
  ...knownServers().map((s) => ({ value: s.id, label: `${s.name} (${s.host}${s.port === 80 ? "" : `:${s.port}`})` })),
];

registerRunConfigurationType({
  id: "php-web-page",
  name: "PHP Web Page",
  description: "Open a page of a web server in the browser; Debug starts listening and adds the Xdebug trigger.",
  icon: Globe,
  color: "var(--info)",
  pluginId: "forge.xdebug",
  argumentsLabel: false,
  debuggable: true,
  fields: [
    { key: "server", label: "Server", kind: "enum", options: serverOptions(false), required: true },
    { key: "startUrl", label: "Start URL", default: "/", placeholder: "/orders/1042" },
  ],
  suggestName: (cfg) => opt(cfg, "startUrl") || undefined,
  start: async (cfg, request) => {
    const started = await debugApi.webPageStart(request.runId!, opt(cfg, "server") || undefined, opt(cfg, "startUrl") || "/", isDebug(request));
    window.open(started.url, "_blank", "noopener");
    return { runId: started.runId, commandLine: started.commandLine };
  },
});

const METHODS = ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];

/** The request as `.http` text, sent by the HTTP client. */
export function httpRequestText(method: string, url: string, headers: string, body: string): string {
  const head = headers
    .split("\n")
    .map((h) => h.trim())
    .filter((h) => h.includes(":"));
  const text = [`${method || "GET"} ${url}`, ...head].join("\n");
  return body.trim() ? `${text}\n\n${body}\n` : `${text}\n`;
}

registerRunConfigurationType({
  id: "php-http-request",
  name: "PHP HTTP Request",
  description: "Send an HTTP request to a web server; Debug starts listening and adds the Xdebug session cookie.",
  icon: Send,
  color: "var(--syn-function)",
  pluginId: "forge.xdebug",
  argumentsLabel: false,
  debuggable: true,
  fields: [
    { key: "server", label: "Server", kind: "enum", options: serverOptions(true) },
    { key: "url", label: "URL", required: true, placeholder: "/api/orders or https://shop.test/api/orders" },
    { key: "method", label: "Request method", kind: "enum", default: "GET", options: METHODS.map((m) => ({ value: m, label: m })) },
    { key: "headers", label: "Headers", kind: "text", placeholder: "Accept: application/json" },
    { key: "body", label: "Request body", kind: "text", placeholder: '{"status": "paid"}' },
  ],
  suggestName: (cfg) => `${opt(cfg, "method") || "GET"} ${opt(cfg, "url")}`.trim(),
  start: async (cfg, request) => {
    const url = await debugApi.pageUrl(opt(cfg, "server") || undefined, opt(cfg, "url"));
    const content = httpRequestText(opt(cfg, "method"), url, opt(cfg, "headers"), opt(cfg, "body"));
    return scratchesApi.httpRun(request.runId!, content, 0, undefined, undefined, isDebug(request));
  },
});
