/** Status bar widget showing the language servers' state, e.g. "Intelephense ✓". */
import { For, Match, Show, Switch } from "solid-js";
import { Braces, Check, CircleAlert, LoaderCircle } from "lucide-solid";
import type { ServerStatus } from "@/ipc/lsp";
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

export function LspStatusItem() {
  const visible = () => servers().filter((s) => s.state !== "notStarted");
  return (
    <Show when={visible().length}>
      <For each={visible()}>
        {(s) => (
          <button class="status-item lsp-status" data-state={s.state} title={statusTooltip(s)}>
            <Braces size={12} />
            <span>{s.name}</span>
            <Switch>
              <Match when={s.state === "starting" || s.state === "busy"}>
                <LoaderCircle size={12} class="lsp-spin" />
                <Show when={s.state === "busy" && s.percentage != null}>
                  <span class="lsp-status-detail">{(s as { percentage: number }).percentage}%</span>
                </Show>
              </Match>
              <Match when={s.state === "ready"}>
                <Check size={12} class="lsp-ok" />
              </Match>
              <Match when={s.state === "failed" || s.state === "stopped"}>
                <CircleAlert size={12} class="lsp-failed" />
              </Match>
            </Switch>
          </button>
        )}
      </For>
    </Show>
  );
}
