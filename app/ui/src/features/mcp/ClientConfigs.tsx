/**
 * "Copy client configuration": the command or JSON each MCP client needs to connect, built
 * in Rust for the running server (URL, bearer token, the stdio proxy's executable).
 */
import { createResource, createSignal, For, onCleanup, Show } from "solid-js";
import { Check, Copy } from "lucide-solid";
import { mcpApi, type McpClientConfig } from "@/ipc/mcp";
import { mcpStatus } from "./store";

async function copyText(text: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(text);
  } catch {
    /* clipboard unavailable; the text stays selectable */
  }
}

export function CopyButton(props: { text: string; label?: string; testId?: string }) {
  const [copied, setCopied] = createSignal(false);
  let timer: ReturnType<typeof setTimeout> | undefined;
  onCleanup(() => clearTimeout(timer));
  return (
    <button
      class="btn-secondary mcp-copy"
      data-testid={props.testId}
      onClick={async () => {
        await copyText(props.text);
        setCopied(true);
        clearTimeout(timer);
        timer = setTimeout(() => setCopied(false), 1500);
      }}
    >
      <Show when={copied()} fallback={<Copy size={13} />}>
        <Check size={13} />
      </Show>
      {copied() ? "Copied" : (props.label ?? "Copy")}
    </button>
  );
}

/** Tabs per client with the snippet and a copy button. */
export function ClientConfigs() {
  const [configs] = createResource(
    () => mcpStatus()?.url ?? null,
    (url) => (url ? mcpApi.clientConfigs() : Promise.resolve(null)),
  );
  const [selected, setSelected] = createSignal("claude-code");
  const current = (): McpClientConfig | undefined => configs()?.find((c) => c.id === selected()) ?? configs()?.[0];
  return (
    <Show when={configs()} fallback={<div class="muted mcp-note">Enable the server to get the client configurations.</div>}>
      <div class="mcp-clients" data-testid="mcp-client-configs">
        <div class="mcp-client-tabs" role="tablist">
          <For each={configs()}>
            {(c) => (
              <button role="tab" class="mcp-client-tab" classList={{ active: current()?.id === c.id }} aria-selected={current()?.id === c.id} onClick={() => setSelected(c.id)}>
                {c.name}
              </button>
            )}
          </For>
        </div>
        <Show when={current()}>
          {(c) => (
            <div class="mcp-code">
              <div class="mcp-code-head">
                <span class="muted">
                  {c().target}
                  <span class="mcp-transport">{c().transport === "http" ? "Streamable HTTP" : "stdio proxy"}</span>
                </span>
                <CopyButton text={c().text} testId="mcp-copy-config" />
              </div>
              <pre class="mono">{c().text}</pre>
            </div>
          )}
        </Show>
        <div class="muted mcp-note">
          The configurations contain the bearer token. Anyone with it can use Forge's tools while the server runs; regenerate it if it leaks.
        </div>
      </div>
    </Show>
  );
}
