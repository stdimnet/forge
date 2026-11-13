/**
 * The host key question, worded like PHPStorm / OpenSSH: an unknown host shows its key
 * fingerprint for confirmation; a changed key is a warning that needs an explicit
 * "Accept New Key". Accepted keys are written to `~/.ssh/known_hosts` by the backend.
 */
import { Match, Show, Switch } from "solid-js";
import { ShieldAlert, ShieldQuestion } from "lucide-solid";
import type { HostKeyInfo } from "@/ipc/remote";
import { hostKeyQuestion } from "./store";
import { t as tr } from "@/i18n";

/** `ssh-ed25519` → `ED25519`, `ecdsa-sha2-nistp256` → `ECDSA`, `ssh-rsa` → `RSA`. */
export function keyTypeLabel(keyType: string): string {
  const t = keyType.replace(/^ssh-/, "").replace(/^sk-/, "");
  if (t.startsWith("ecdsa")) return "ECDSA";
  return t.split("@")[0].toUpperCase();
}

const hostName = (i: HostKeyInfo) => (i.port === 22 ? i.host : `[${i.host}]:${i.port}`);

export function HostKeyDialog() {
  return (
    <Show when={hostKeyQuestion()}>
      {(q) => {
        const info = () => q().info;
        const changed = () => info().status === "changed";
        return (
          <div class="ssh-overlay ssh-overlay-top">
            <div
              class="ssh-dialog ssh-hostkey"
              classList={{ danger: changed() }}
              role="alertdialog"
              aria-label={changed() ? tr("tools.remote.hostKeyHasChanged") : tr("tools.remote.unknownHost")}
              data-testid="ssh-hostkey"
              onKeyDown={(e) => e.key === "Escape" && q().answer(false)}
            >
              <div class="ssh-titlebar">
                <span>{changed() ? tr("tools.remote.hostKeyHasChanged") : tr("tools.remote.unknownHost")}</span>
              </div>
              <div class="ssh-hostkey-body">
                <Switch>
                  <Match when={changed()}>
                    <ShieldAlert size={34} class="ssh-bad ssh-hostkey-icon" />
                  </Match>
                  <Match when={true}>
                    <ShieldQuestion size={34} class="ssh-warn ssh-hostkey-icon" />
                  </Match>
                </Switch>
                <div class="ssh-hostkey-text">
                  <Switch>
                    <Match when={changed()}>
                      <p class="ssh-hostkey-lead">{tr("tools.remote.theHostKeyOfHas", { value: hostName(info()) })}</p>
                      <p>
                        {tr("tools.remote.someoneCouldBeEavesdroppingOn")}
                      </p>
                    </Match>
                    <Match when={true}>
                      <p class="ssh-hostkey-lead">{tr("tools.remote.theAuthenticityOfHostCan", { value: hostName(info()) })}</p>
                    </Match>
                  </Switch>
                  <p>
                    {tr("tools.remote.keyFingerprintIsLabel", { value: changed() ? tr("tools.remote.new") : "", value2: keyTypeLabel(info().keyType) })}
                  </p>
                  <code class="ssh-fingerprint" data-testid="ssh-fingerprint">
                    {info().fingerprint}
                  </code>
                  <Show when={info().status === "changed" && (info() as Extract<HostKeyInfo, { status: "changed" }>)}>
                    {(c) => (
                      <p class="muted ssh-small">
                        {tr("tools.remote.oldFingerprintLabel")} <code>{c().oldFingerprint}</code>
                        <br />
                        {tr("tools.remote.offendingKeyInSshKnown", { value: c().line + 1 })}
                      </p>
                    )}
                  </Show>
                  <p>{changed() ? tr("tools.remote.doYouWantToReplace") : tr("tools.remote.areYouSureYouWant")}</p>
                </div>
              </div>
              <div class="ssh-footer">
                <span class="ssh-progress" />
                <Switch>
                  <Match when={changed()}>
                    <button class="btn-secondary" onClick={() => q().answer(false)} autofocus>
                      {tr("common.cancel")}
                    </button>
                    <button class="btn-primary ssh-danger-btn" onClick={() => q().answer(true)}>
                      {tr("tools.remote.acceptNewKey")}
                    </button>
                  </Match>
                  <Match when={true}>
                    <button class="btn-secondary" onClick={() => q().answer(false)}>
                      {tr("tools.remote.no")}
                    </button>
                    <button class="btn-primary" onClick={() => q().answer(true)} autofocus data-testid="ssh-hostkey-yes">
                      {tr("tools.remote.yes")}
                    </button>
                  </Match>
                </Switch>
              </div>
            </div>
          </div>
        );
      }}
    </Show>
  );
}
