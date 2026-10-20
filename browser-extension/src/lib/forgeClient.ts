/**
 * Client for Forge's local Browser Companion API (`http://127.0.0.1:<port>/api/forge/*`).
 * Forge listens on 63342 or, if that port is taken (e.g. by PhpStorm), one of the next nine.
 */
import type { OpenTarget, PingResult } from "./types";

export const DEFAULT_PORT = 63342;
export const PORT_RANGE = 10;
const TIMEOUT_MS = 1500;

export class ForgeError extends Error {
  constructor(
    message: string,
    readonly status = 0,
  ) {
    super(message);
  }
}

export interface Discovered {
  port: number;
  ping: PingResult;
}

export interface OpenOutcome {
  status: "opened" | "needsConfirmation";
  file: string;
  line?: number | null;
}

type Fetch = typeof fetch;

/** Ports to try: the remembered one first, then the default range. */
export function candidatePorts(preferred?: number): number[] {
  const range = Array.from({ length: PORT_RANGE }, (_, i) => DEFAULT_PORT + i);
  return preferred ? [preferred, ...range.filter((p) => p !== preferred)] : range;
}

export class ForgeClient {
  constructor(private readonly fetchImpl: Fetch = (...args) => fetch(...args)) {}

  private async request<T>(port: number, path: string, init: RequestInit & { token?: string } = {}): Promise<T> {
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), TIMEOUT_MS);
    const headers: Record<string, string> = {};
    if (init.body) headers["Content-Type"] = "application/json";
    if (init.token) headers.Authorization = `Bearer ${init.token}`;
    let res: Response;
    try {
      res = await this.fetchImpl(`http://127.0.0.1:${port}${path}`, {
        method: init.method ?? "GET",
        body: init.body,
        headers,
        signal: controller.signal,
        credentials: "omit",
        cache: "no-store",
      });
    } catch {
      throw new ForgeError("Forge is not running", 0);
    } finally {
      clearTimeout(timer);
    }
    const data = (await res.json().catch(() => ({}))) as T & { error?: string };
    if (!res.ok) throw new ForgeError(data.error ?? `HTTP ${res.status}`, res.status);
    return data;
  }

  /** Ping one port; null if nothing (or not Forge) answers there. */
  async ping(port: number, token?: string): Promise<PingResult | null> {
    try {
      const ping = await this.request<PingResult>(port, "/api/forge/ping", { token });
      return ping.name === "Forge" ? ping : null;
    } catch {
      return null;
    }
  }

  /** Find a running Forge, trying the remembered port first. */
  async discover(preferredPort?: number, token?: string): Promise<Discovered | null> {
    for (const port of candidatePorts(preferredPort)) {
      const ping = await this.ping(port, token);
      if (ping) return { port, ping };
    }
    return null;
  }

  pair(port: number, code: string, name: string): Promise<{ clientId: string; token: string }> {
    return this.request(port, "/api/forge/pair", {
      method: "POST",
      body: JSON.stringify({ code: code.replace(/\D/g, ""), name }),
    });
  }

  open(port: number, token: string, target: OpenTarget): Promise<OpenOutcome> {
    return this.request(port, "/api/forge/open", { method: "POST", token, body: JSON.stringify(target) });
  }

  openSymbol(port: number, token: string, name: string, kind: "class" | "component" = "component"): Promise<OpenOutcome> {
    return this.request(port, "/api/forge/open-symbol", { method: "POST", token, body: JSON.stringify({ name, kind }) });
  }
}

/** A human-readable browser name for the pairing list in Forge, e.g. "Chrome 141 on Linux". */
export function describeBrowser(userAgent: string): string {
  const version = (re: RegExp) => re.exec(userAgent)?.[1];
  const [edge, firefox, chrome] = [version(/Edg\/(\d+)/), version(/Firefox\/(\d+)/), version(/Chrome\/(\d+)/)];
  const browser = edge ? `Edge ${edge}` : firefox ? `Firefox ${firefox}` : chrome ? `Chrome ${chrome}` : "Browser";
  const os = /Windows/.test(userAgent)
    ? "Windows"
    : /Mac OS X|Macintosh/.test(userAgent)
      ? "macOS"
      : /Linux|X11/.test(userAgent)
        ? "Linux"
        : "";
  return os ? `${browser} on ${os}` : browser;
}
