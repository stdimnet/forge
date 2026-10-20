/**
 * The SSH connection fields shared by the New SSH Connection dialog and Settings | Tools |
 * SSH Configurations: host (with `~/.ssh/config` aliases), port, user, authentication.
 */
import { createResource, For, Match, Show, Switch } from "solid-js";
import { Folder } from "lucide-solid";
import { remoteApi, type AuthMethod, type SshConfiguration } from "@/ipc/remote";
import { isTauri } from "@/ipc";

export const AUTH_LABELS: Record<AuthMethod, string> = {
  password: "Password",
  keyPair: "Key pair (OpenSSH or PuTTY)",
  agent: "OpenSSH config and authentication agent",
};

let formSeq = 0;

export function SshForm(props: {
  config: SshConfiguration;
  update: (patch: Partial<SshConfiguration>) => void;
  secret: string;
  setSecret: (s: string) => void;
  /** A secret is stored in the keychain for this configuration. */
  hasSecret?: boolean;
  problems?: string[];
  autofocus?: boolean;
}) {
  const id = `ssh-form-${++formSeq}`;
  const [hosts] = createResource(() => remoteApi.sshHosts().catch(() => []));
  const problem = (word: string) => props.problems?.find((p) => p.toLowerCase().includes(word));

  const browseKey = async () => {
    if (!isTauri()) return;
    const { open } = await import("@tauri-apps/plugin-dialog");
    const path = await open({ multiple: false, title: "Select Private Key File" });
    if (typeof path === "string") props.update({ keyFile: path });
  };

  /** Picking a `~/.ssh/config` alias fills in its user and port. */
  const onHost = (value: string) => {
    const entry = hosts()?.find((h) => h.alias === value);
    props.update({
      host: value,
      ...(entry?.user && !props.config.user ? { user: entry.user } : {}),
      ...(entry?.port ? { port: entry.port } : {}),
    });
  };

  return (
    <div class="ssh-form">
      <label class="ssh-label" for={`${id}-host`}>
        Host:
      </label>
      <div class="ssh-host-row">
        <input
          id={`${id}-host`}
          class="ssh-input"
          classList={{ invalid: !!problem("host") }}
          list={`${id}-hosts`}
          placeholder="example.com, 10.0.0.5 or a Host from ~/.ssh/config"
          value={props.config.host}
          spellcheck={false}
          autofocus={props.autofocus}
          onInput={(e) => onHost(e.currentTarget.value)}
          data-testid="ssh-host"
        />
        <datalist id={`${id}-hosts`}>
          <For each={hosts() ?? []}>
            {(h) => <option value={h.alias}>{`${h.user ? `${h.user}@` : ""}${h.hostName}${h.port ? `:${h.port}` : ""}`}</option>}
          </For>
        </datalist>
        <label class="ssh-label inline" for={`${id}-port`}>
          Port:
        </label>
        <input
          id={`${id}-port`}
          class="ssh-input ssh-port"
          classList={{ invalid: !!problem("port") }}
          type="number"
          min="1"
          max="65535"
          value={props.config.port}
          onInput={(e) => props.update({ port: Number(e.currentTarget.value) || 0 })}
        />
      </div>

      <label class="ssh-label" for={`${id}-user`}>
        Username:
      </label>
      <input
        id={`${id}-user`}
        class="ssh-input"
        classList={{ invalid: !!problem("user") }}
        value={props.config.user}
        spellcheck={false}
        onInput={(e) => props.update({ user: e.currentTarget.value })}
        data-testid="ssh-user"
      />

      <label class="ssh-label" for={`${id}-auth`}>
        Authentication type:
      </label>
      <select
        id={`${id}-auth`}
        class="ssh-input"
        value={props.config.auth}
        onChange={(e) => props.update({ auth: e.currentTarget.value as AuthMethod })}
      >
        <For each={Object.entries(AUTH_LABELS)}>{([value, label]) => <option value={value}>{label}</option>}</For>
      </select>

      <Switch>
        <Match when={props.config.auth === "password"}>
          <label class="ssh-label" for={`${id}-secret`}>
            Password:
          </label>
          <div class="ssh-secret-row">
            <input
              id={`${id}-secret`}
              class="ssh-input"
              type="password"
              placeholder={props.hasSecret ? "Stored in the system keychain" : ""}
              value={props.secret}
              onInput={(e) => props.setSecret(e.currentTarget.value)}
              data-testid="ssh-password"
            />
            <label class="ssh-check">
              <input
                type="checkbox"
                checked={!props.config.dontSaveSecret}
                onChange={(e) => props.update({ dontSaveSecret: !e.currentTarget.checked })}
              />
              Save password
            </label>
          </div>
        </Match>
        <Match when={props.config.auth === "keyPair"}>
          <label class="ssh-label" for={`${id}-key`}>
            Private key file:
          </label>
          <div class="ssh-path">
            <input
              id={`${id}-key`}
              class="ssh-input"
              classList={{ invalid: !!problem("key") }}
              placeholder="~/.ssh/id_ed25519"
              value={props.config.keyFile ?? ""}
              spellcheck={false}
              onInput={(e) => props.update({ keyFile: e.currentTarget.value })}
            />
            <button
              class="ssh-browse"
              title={isTauri() ? "Browse…" : "The file chooser is available in the desktop app"}
              disabled={!isTauri()}
              onClick={() => void browseKey()}
            >
              <Folder size={14} />
            </button>
          </div>
          <label class="ssh-label" for={`${id}-secret`}>
            Passphrase:
          </label>
          <div class="ssh-secret-row">
            <input
              id={`${id}-secret`}
              class="ssh-input"
              type="password"
              placeholder={props.hasSecret ? "Stored in the system keychain" : "Empty for keys without a passphrase"}
              value={props.secret}
              onInput={(e) => props.setSecret(e.currentTarget.value)}
            />
            <label class="ssh-check">
              <input
                type="checkbox"
                checked={!props.config.dontSaveSecret}
                onChange={(e) => props.update({ dontSaveSecret: !e.currentTarget.checked })}
              />
              Save passphrase
            </label>
          </div>
        </Match>
        <Match when={props.config.auth === "agent"}>
          <span />
          <p class="ssh-hint">
            Uses the keys of the running ssh-agent, the <code>IdentityFile</code> entries of <code>~/.ssh/config</code> and
            the default keys in <code>~/.ssh</code>.
          </p>
        </Match>
      </Switch>

      <Show when={props.problems?.length}>
        <span />
        <ul class="ssh-problems">
          <For each={props.problems}>{(p) => <li>{p}</li>}</For>
        </ul>
      </Show>
    </div>
  );
}
