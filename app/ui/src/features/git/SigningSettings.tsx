/**
 * Settings | Version Control | Git › "Sign commits with GPG/SSH key" with its Configure GPG
 * Key… dialog: the keys come from `gpg --list-secret-keys` and `~/.ssh/*.pub` on the
 * project's machine (`git_signing_info`), git config is shown read-only, and the choice is
 * stored in the Forge setting `vcs.git.commit.signingKey` (applied per commit with `-c`).
 */
import { createResource, createSignal, For, onCleanup, Show } from "solid-js";
import { KeyRound, LoaderCircle } from "lucide-solid";
import type { SettingSpec } from "@/ipc/settings";
import { suspendKeymap } from "@/keymap";
import { draftValue, setDraft } from "@/features/settings/dialogState";
import { Hl } from "@/features/settings/dialogSearch";
import { extrasApi } from "./extrasApi";
import "./extras.css";

export const SIGN_KEY = "vcs.git.commit.gpgSign";
export const SIGNING_KEY = "vcs.git.commit.signingKey";

/** `openpgp:<id>` → `GPG key <id>`, `ssh:<path>` → `SSH key <path>`. */
export function describeSigningKey(value: string): string {
  const v = value.trim();
  if (!v) return "Key from git config (user.signingkey)";
  const [prefix, ...rest] = v.split(":");
  const key = rest.join(":");
  if (prefix === "ssh") return `SSH key ${key}`;
  if (prefix === "openpgp") return `GPG key ${key}`;
  if (prefix === "x509") return `X.509 key ${key}`;
  return `GPG key ${v}`;
}

function ConfigureDialog(props: { value: string; onClose: (value: string | null) => void }) {
  const [info] = createResource(() => extrasApi.signingInfo());
  const [choice, setChoice] = createSignal(props.value);
  const release = suspendKeymap();
  onCleanup(release);
  const onKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Escape") props.onClose(null);
    else if (e.key === "Enter") props.onClose(choice());
    else return;
    e.preventDefault();
    e.stopPropagation();
  };
  const Option = (o: { value: string; title: string; sub?: string; invalid?: boolean }) => (
    <label class="signing-option" classList={{ invalid: o.invalid }} data-signing-key={o.value}>
      <input type="radio" name="signing-key" checked={choice() === o.value} disabled={o.invalid} onChange={() => setChoice(o.value)} />
      <span class="signing-option-text">
        <span>{o.title}</span>
        <Show when={o.sub}>
          <span class="signing-option-sub">{o.sub}</span>
        </Show>
      </span>
    </label>
  );
  return (
    <div class="signing-dialog" onMouseDown={(e) => e.target === e.currentTarget && props.onClose(null)}>
      <div class="signing-box" role="dialog" aria-label="Configure GPG Key" onKeyDown={onKeyDown} tabIndex={-1} ref={(el) => queueMicrotask(() => el.focus())}>
        <header>Configure GPG Key</header>
        <div class="signing-body">
          <Show when={!info.loading} fallback={<span class="muted"><LoaderCircle size={13} class="spin" /> Looking for keys…</span>}>
            <Show when={info()} fallback={<span class="gd-invalid">{String((info.error as Error)?.message ?? info.error ?? "")}</span>}>
              {(i) => (
                <>
                  <div class="signing-config" data-testid="signing-config">
                    Git config{i().remote ? " on the remote host" : ""} (read-only): user.signingkey = {i().config.signingKey ?? "not set"}, gpg.format ={" "}
                    {i().config.format ?? "openpgp"}, commit.gpgsign = {String(i().config.gpgSign)}
                  </div>
                  <Option value="" title="Use the key from git config" sub={i().config.signingKey ?? "user.signingkey is not set: git uses the committer's default key"} />
                  <div class="signing-group">GPG keys</div>
                  <Show when={!i().gpgError} fallback={<span class="gd-note">{i().gpgError}</span>}>
                    <For each={i().gpgKeys} fallback={<span class="gd-note">No GPG secret keys</span>}>
                      {(k) => (
                        <Option
                          value={`openpgp:${k.id}`}
                          title={k.userIds[0] ?? k.id}
                          sub={`${k.id}${k.expires ? ` · expires ${new Date(k.expires * 1000).toLocaleDateString()}` : ""}${k.invalid ? " · expired or revoked" : ""}`}
                          invalid={k.invalid || !k.canSign}
                        />
                      )}
                    </For>
                  </Show>
                  <div class="signing-group">SSH keys</div>
                  <For each={i().sshKeys} fallback={<span class="gd-note">No public keys in ~/.ssh</span>}>
                    {(k) => <Option value={`ssh:${k.path}`} title={k.comment || k.path} sub={`${k.keyType} · ${k.path}`} />}
                  </For>
                </>
              )}
            </Show>
          </Show>
        </div>
        <footer>
          <button class="btn-secondary" onClick={() => props.onClose(null)}>
            Cancel
          </button>
          <button class="btn-primary" onClick={() => props.onClose(choice())} data-testid="signing-ok">
            OK
          </button>
        </footer>
      </div>
    </div>
  );
}

/** The row of `vcs.git.commit.gpgSign`: checkbox, chosen key and "Configure GPG Key…". */
export function SigningRow(props: { spec: SettingSpec }) {
  const [open, setOpen] = createSignal(false);
  const key = () => draftValue<string>(SIGNING_KEY) ?? "";
  return (
    <div class="signing-row" data-testid="signing-row">
      <div class="signing-line">
        <label class="set-check">
          <input type="checkbox" checked={draftValue<boolean>(props.spec.key)} onChange={(e) => setDraft(props.spec.key, e.currentTarget.checked)} />
          <span>
            <Hl text={props.spec.title} />
          </span>
        </label>
        <button class="btn-secondary" onClick={() => setOpen(true)} data-testid="signing-configure">
          <KeyRound size={13} /> Configure GPG Key…
        </button>
      </div>
      <div class="set-indent">
        <span class="signing-key">{describeSigningKey(key())}</span>
        <Show when={props.spec.description}>
          <div class="set-desc">
            <Hl text={props.spec.description!} />
          </div>
        </Show>
      </div>
      <Show when={open()}>
        <ConfigureDialog
          value={key()}
          onClose={(value) => {
            setOpen(false);
            if (value === null) return;
            setDraft(SIGNING_KEY, value);
            setDraft(props.spec.key, true);
          }}
        />
      </Show>
    </div>
  );
}
