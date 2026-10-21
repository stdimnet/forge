/**
 * Settings | Tools | Code With Me | Relay Server: the relay token (kept in the OS keychain,
 * never in a settings file) and Test Connection, below the "Relay server" settings.
 */
import { createSignal, onMount, Show } from "solid-js";
import { Portal } from "solid-js/web";
import { Eye, EyeOff, X } from "lucide-solid";
import { collabApi, type RelayStatus, type RelayTokenStatus } from "@/ipc/collab";
import { draftValue } from "@/features/settings/dialogState";
import { relayHost } from "./connection";

const errorText = (e: unknown) => String((e as Error)?.message ?? e);

function TokenDialog(props: { current: RelayTokenStatus | null; onClose: (s?: RelayTokenStatus) => void }) {
  const [token, setToken] = createSignal("");
  const [reveal, setReveal] = createSignal(false);
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  let input!: HTMLInputElement;
  onMount(() => input.focus());
  const save = async (value: string | null) => {
    setBusy(true);
    setError(null);
    try {
      props.onClose(await collabApi.setRelayToken(value));
    } catch (e) {
      setError(errorText(e));
      setBusy(false);
    }
  };
  return (
    <Portal>
      <div class="cwm-overlay" onMouseDown={(e) => e.target === e.currentTarget && props.onClose()}>
        <div class="cwm-dialog" role="dialog" aria-label="Relay Token" onKeyDown={(e) => e.key === "Escape" && props.onClose()}>
          <header class="cwm-dialog-title">
            <span>Relay Token</span>
            <button class="icon-btn" title="Close" onClick={() => props.onClose()}>
              <X size={15} />
            </button>
          </header>
          <div class="cwm-dialog-body">
            <p class="cwm-dialog-text">
              Relay servers started with token access control only accept connections that present the token. Guests who use the
              same relay server enter the same token.
            </p>
            <label for="cwm-relay-token">Token:</label>
            <div class="cwm-secret">
              <input
                id="cwm-relay-token"
                ref={input}
                class="cwm-input"
                type={reveal() ? "text" : "password"}
                autocomplete="off"
                spellcheck={false}
                placeholder={props.current?.set ? "Replace the stored token" : "Paste the token"}
                value={token()}
                onInput={(e) => setToken(e.currentTarget.value)}
                onKeyDown={(e) => e.key === "Enter" && token().trim() && void save(token())}
              />
              <button class="icon-btn" title={reveal() ? "Hide" : "Show"} onClick={() => setReveal((v) => !v)}>
                {reveal() ? <EyeOff size={14} /> : <Eye size={14} />}
              </button>
            </div>
            <Show when={error()}>
              <div class="cwm-error">{error()}</div>
            </Show>
            <p class="cwm-hint">Stored in the system keychain, never in settings files or session links.</p>
          </div>
          <footer class="cwm-dialog-footer">
            <Show when={props.current?.set}>
              <button class="btn-secondary cwm-remove" disabled={busy()} onClick={() => void save(null)}>
                Remove Token
              </button>
            </Show>
            <span class="cwm-spacer" />
            <button class="btn-secondary" onClick={() => props.onClose()}>
              Cancel
            </button>
            <button class="btn-primary" disabled={busy() || !token().trim()} onClick={() => void save(token())}>
              Save
            </button>
          </footer>
        </div>
      </div>
    </Portal>
  );
}

export function RelayServerTools() {
  const [status, setStatus] = createSignal<RelayTokenStatus | null>(null);
  const [dialog, setDialog] = createSignal(false);
  const [testing, setTesting] = createSignal(false);
  const [result, setResult] = createSignal<{ ok: boolean; text: string } | null>(null);
  onMount(() => void collabApi.relayTokenStatus().then(setStatus, () => setStatus(null)));

  const url = () => (draftValue<string>("codeWithMe.relayUrl") ?? "").trim();
  const publicRelays = () => !!draftValue<boolean>("codeWithMe.usePublicRelays");
  const test = async () => {
    setTesting(true);
    setResult(null);
    try {
      const r: RelayStatus = await collabApi.testRelay(url(), publicRelays());
      setResult({ ok: r.connected, text: `Connected to ${relayHost(r.url)}.` });
    } catch (e) {
      setResult({ ok: false, text: errorText(e) });
    } finally {
      setTesting(false);
    }
  };
  return (
    <div class="cwm-relay-tools" data-testid="cwm-relay-tools">
      <div class="cwm-relay-row">
        <span class="cwm-relay-label">Relay token:</span>
        <span class="cwm-relay-token" classList={{ set: !!status()?.set }} data-token={status()?.set ? "set" : "none"}>
          {status()?.set ? "Stored in the keychain" : "None"}
        </span>
        <button class="btn-secondary cwm-small-btn" disabled={!url()} onClick={() => setDialog(true)}>
          {status()?.set ? "Change Token…" : "Set Token…"}
        </button>
        <button class="btn-secondary cwm-small-btn" disabled={testing() || (!url() && !publicRelays())} onClick={() => void test()}>
          {testing() ? "Testing…" : "Test Connection"}
        </button>
      </div>
      <Show when={status()?.error}>
        <div class="cwm-error">Keychain: {status()?.error}</div>
      </Show>
      <Show when={result()}>
        {(r) => (
          <div class="cwm-test-result" classList={{ ok: r().ok, fail: !r().ok }} data-testid="cwm-relay-test">
            {r().text}
          </div>
        )}
      </Show>
      <Show when={dialog()}>
        <TokenDialog
          current={status()}
          onClose={(s) => {
            setDialog(false);
            if (s) setStatus(s);
          }}
        />
      </Show>
    </div>
  );
}
