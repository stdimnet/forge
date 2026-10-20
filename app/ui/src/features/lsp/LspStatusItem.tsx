/**
 * Status bar widget for the language servers: "Indexing… TypeScript 45%" while a server
 * reports progress, otherwise the server (or the number of servers) with its state.
 * Clicking it opens Settings | Languages & Frameworks | Language Servers.
 */
import { createMemo, Match, Show, Switch } from "solid-js";
import { Braces, Check, CircleAlert, LoaderCircle } from "lucide-solid";
import type { ServerStatus } from "@/ipc/lsp";
import { openSettings } from "@/features/settings/dialogState";
import { servers } from "./store";

export function statusTooltip(s: ServerStatus): string {
  switch (s.state) {
    case "notStarted":
      return `${s.name}: not started (opens with the first matching file)`;
    case "starting":
      return `${s.name}: starting…`;
    case "ready":
      return `${s.name}: running`;
    case "busy":
      return `${s.name}: ${s.message}${s.percentage != null ? ` (${s.percentage}%)` : ""}`;
    case "stopped":
      return `${s.name}: stopped`;
    case "failed":
      return `${s.name}: failed to start — ${s.error}`;
  }
}

/** "TypeScript Language Server" → "TypeScript" for the narrow status bar. */
export function shortName(name: string): string {
  const short = name
    .replace(/\s*\(.*\)$/, "")
    .replace(/\s+(Language Server|Language Service|IntelliSense)$/i, "")
    .trim();
  return short || name;
}

type Summary =
  | { kind: "busy"; server: ServerStatus & { state: "busy" } }
  | { kind: "starting"; server: ServerStatus }
  | { kind: "failed"; server: ServerStatus }
  | { kind: "ready"; server: ServerStatus; count: number };

/** What the widget shows for a set of servers (most urgent first). */
export function summarize(list: ServerStatus[]): Summary | null {
  const visible = list.filter((s) => s.state !== "notStarted" && s.name);
  if (!visible.length) return null;
  const busy = visible.find((s): s is ServerStatus & { state: "busy" } => s.state === "busy");
  if (busy) return { kind: "busy", server: busy };
  const starting = visible.find((s) => s.state === "starting");
  if (starting) return { kind: "starting", server: starting };
  const failed = visible.find((s) => s.state === "failed");
  if (failed) return { kind: "failed", server: failed };
  const ready = visible.filter((s) => s.state === "ready");
  if (!ready.length) return { kind: "failed", server: visible[0] };
  return { kind: "ready", server: ready[0], count: ready.length };
}

export function LspStatusItem() {
  const summary = createMemo(() => summarize(servers()));
  const tooltip = () =>
    servers()
      .filter((s) => s.state !== "notStarted" && s.name)
      .map(statusTooltip)
      .join("\n");
  return (
    <Show when={summary()}>
      {(s) => (
        <button
          class="status-item lsp-status"
          data-state={s().server.state}
          title={`${tooltip()}\n\nClick to manage language servers`}
          onClick={() => openSettings("lang.servers")}
        >
          <Switch>
            <Match when={s().kind === "busy" && (s() as Extract<Summary, { kind: "busy" }>)}>
              {(b) => (
                <>
                  <LoaderCircle size={12} class="lsp-spin" />
                  <span>
                    {b().server.message || "Indexing"}… {shortName(b().server.name)}
                  </span>
                  <Show when={b().server.percentage != null}>
                    <span class="lsp-status-detail">{b().server.percentage}%</span>
                  </Show>
                </>
              )}
            </Match>
            <Match when={s().kind === "starting"}>
              <LoaderCircle size={12} class="lsp-spin" />
              <span>Starting {shortName(s().server.name)}…</span>
            </Match>
            <Match when={s().kind === "failed"}>
              <Braces size={12} />
              <span>{shortName(s().server.name)}</span>
              <CircleAlert size={12} class="lsp-failed" />
            </Match>
            <Match when={s().kind === "ready" && (s() as Extract<Summary, { kind: "ready" }>)}>
              {(r) => (
                <>
                  <Braces size={12} />
                  <span>{r().count > 1 ? `${r().count} language servers` : r().server.name}</span>
                  <Check size={12} class="lsp-ok" />
                </>
              )}
            </Match>
          </Switch>
        </button>
      )}
    </Show>
  );
}
