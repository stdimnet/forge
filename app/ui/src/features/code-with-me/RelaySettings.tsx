/**
 * Settings | Tools | Code With Me | Relay Server: the relay token (kept in the OS keychain,
 * never in a settings file) and Test Connection, below the "Relay server" settings.
 */
import { createSignal, onMount, Show } from "solid-js";
import { Layer } from "@/shell/layers";
import { Eye, EyeOff, X } from "lucide-solid";
import { collabApi, type RelayStatus, type RelayTokenStatus } from "@/ipc/collab";
import { draftValue } from "@/features/settings/dialogState";
import { relayHost } from "./connection";
import { t } from "@/i18n";

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
    <Layer kind="modal">
      <div class="cwm-overlay" onMouseDown={(e) => e.target === e.currentTarget && props.onClose()}>
        <div class="cwm-dialog" role="dialog" aria-label={t("tools.codeWithMe.relayToken")} onKeyDown={(e) => e.key === "Escape" && props.onClose()}>
          <header class="cwm-dialog-title">
            <span>{t("tools.codeWithMe.relayToken")}</span>
            <button class="icon-btn" title={t("common.close")} onClick={() => props.onClose()}>
              <X size={15} />
            </button>
          </header>
          <div class="cwm-dialog-body">
            <p class="cwm-dialog-text">{t("tools.codeWithMe.relayTokenText")}</p>
            <label for="cwm-relay-token">{t("tools.codeWithMe.tokenLabel")}</label>
            <div class="cwm-secret">
              <input
                id="cwm-relay-token"
                ref={input}
                class="cwm-input"
                type={reveal() ? "text" : "password"}
                autocomplete="off"
                spellcheck={false}
                placeholder={props.current?.set ? t("tools.codeWithMe.replaceStoredToken") : t("tools.codeWithMe.pasteToken")}
                value={token()}
                onInput={(e) => setToken(e.currentTarget.value)}
                onKeyDown={(e) => e.key === "Enter" && token().trim() && void save(token())}
              />
              <button class="icon-btn" title={reveal() ? t("tools.codeWithMe.hide") : t("tools.codeWithMe.show")} onClick={() => setReveal((v) => !v)}>
                {reveal() ? <EyeOff size={14} /> : <Eye size={14} />}
              </button>
            </div>
            <Show when={error()}>
              <div class="cwm-error">{error()}</div>
            </Show>
            <p class="cwm-hint">{t("tools.codeWithMe.tokenStorageHint")}</p>
          </div>
          <footer class="cwm-dialog-footer">
            <Show when={props.current?.set}>
              <button class="btn-secondary cwm-remove" disabled={busy()} onClick={() => void save(null)}>
                {t("tools.codeWithMe.removeToken")}
              </button>
            </Show>
            <span class="cwm-spacer" />
            <button class="btn-secondary" onClick={() => props.onClose()}>
              {t("common.cancel")}
            </button>
            <button class="btn-primary" disabled={busy() || !token().trim()} onClick={() => void save(token())}>
              {t("common.save")}
            </button>
          </footer>
        </div>
      </div>
    </Layer>
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
      setResult({ ok: r.connected, text: t("tools.codeWithMe.connectedTo", { host: relayHost(r.url) }) });
    } catch (e) {
      setResult({ ok: false, text: errorText(e) });
    } finally {
      setTesting(false);
    }
  };
  return (
    <div class="cwm-relay-tools" data-testid="cwm-relay-tools">
      <div class="cwm-relay-row">
        <span class="cwm-relay-label">{t("tools.codeWithMe.relayTokenLabel")}</span>
        <span class="cwm-relay-token" classList={{ set: !!status()?.set }} data-token={status()?.set ? "set" : "none"}>
          {status()?.set ? t("tools.codeWithMe.storedInKeychain") : t("tools.codeWithMe.none")}
        </span>
        <button class="btn-secondary cwm-small-btn" disabled={!url()} onClick={() => setDialog(true)}>
          {status()?.set ? t("tools.codeWithMe.changeToken") : t("tools.codeWithMe.setToken")}
        </button>
        <button class="btn-secondary cwm-small-btn" disabled={testing() || (!url() && !publicRelays())} onClick={() => void test()}>
          {testing() ? t("tools.codeWithMe.testing") : t("tools.codeWithMe.testConnection")}
        </button>
      </div>
      <Show when={status()?.error}>
        <div class="cwm-error">{t("tools.codeWithMe.keychainError", { error: status()?.error })}</div>
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
