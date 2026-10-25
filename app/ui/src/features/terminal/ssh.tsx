/**
 * SSH terminal sessions (Tools | Start SSH Session…, the Terminal's "SSH Session…"): a
 * chooser of the saved SSH configurations and the SFTP deployment servers plus "Edit
 * credentials…", then a Terminal tab connected over SSH (a pseudo terminal on an SSH
 * channel, `forge_remote::shell`) named after the host.
 *
 * Safe Mode: a deployment server is defined in the project's files, so in an untrusted
 * project its session starts only after the user confirmed it (Rust refuses it otherwise).
 * The user's own SSH configurations connect right away.
 */
import { createEffect, createMemo, createSignal, For, on, onMount, Show } from "solid-js";
import { createStore } from "solid-js/store";
import { render } from "solid-js/web";
import { KeyRound, Pencil, Server, X } from "lucide-solid";
import { remoteApi, type SshConfiguration, type SshConfigView } from "@/ipc/remote";
import { deploymentApi, type ServerView } from "@/ipc/deployment";
import type { SshTarget } from "@/ipc/terminal";
import { showBalloon } from "@/keymap";
import { askConfirm } from "@/features/contextMenu/dialogs";
import { safeMode } from "@/features/trust/store";
import { SshForm } from "@/features/remote/SshForm";
import { blankConfig } from "@/features/remote/store";
import { testConnection } from "@/features/remote/TestConnection";
import { isDialogWindow } from "@/shell/window/mode";
import "@/features/remote/remote.css";

/** One entry of the chooser. */
export interface SshChoice {
  kind: "config" | "server";
  id: string;
  title: string;
  /** `user@host[:port]`. */
  detail: string;
  /** The tab title: the host name. */
  host: string;
  config?: SshConfigView;
  server?: ServerView;
}

/** The chooser's entries: SSH configurations, then SFTP deployment servers with a configuration. */
export function sshChoices(configs: SshConfigView[], servers: ServerView[]): SshChoice[] {
  const out: SshChoice[] = configs.map((c) => ({
    kind: "config",
    id: c.id,
    title: c.name || c.label,
    detail: c.label,
    host: c.host,
    config: c,
  }));
  for (const s of servers) {
    if (s.kind !== "sftp" || !s.sshConfig) continue;
    const c = configs.find((x) => x.id === s.sshConfig);
    out.push({ kind: "server", id: s.id, title: s.name, detail: c?.label ?? s.sshConfig, host: c?.host ?? s.name, config: c, server: s });
  }
  return out;
}

/** The text the tab shows for an SSH session: the host, without the domain of an address. */
export const sshTabTitle = (host: string) => host.trim() || "SSH";

// ---- Chooser ---------------------------------------------------------------------------------

interface ChooserRequest {
  choices: SshChoice[];
  pick: (c: SshChoice | "credentials" | null) => void;
}

const [chooser, setChooser] = createSignal<ChooserRequest | null>(null);

function Chooser(props: { request: ChooserRequest }) {
  let popup!: HTMLDivElement;
  const [index, setIndex] = createSignal(0);
  const count = () => props.request.choices.length + 1;
  onMount(() => popup.focus());
  const pickAt = (i: number) => props.request.pick(i < props.request.choices.length ? props.request.choices[i] : "credentials");
  const onKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Escape") props.request.pick(null);
    else if (e.key === "ArrowDown") setIndex((i) => (i + 1) % count());
    else if (e.key === "ArrowUp") setIndex((i) => (i - 1 + count()) % count());
    else if (e.key === "Enter") pickAt(index());
    else return;
    e.preventDefault();
    e.stopPropagation();
  };
  const servers = createMemo(() => props.request.choices.filter((c) => c.kind === "server"));
  const configs = createMemo(() => props.request.choices.filter((c) => c.kind === "config"));
  const row = (c: SshChoice, i: () => number) => (
    <div
      class="term-ssh-item"
      classList={{ selected: index() === i() }}
      role="option"
      aria-selected={index() === i()}
      data-ssh-choice={`${c.kind}:${c.id}`}
      onMouseEnter={() => setIndex(i())}
      onClick={() => props.request.pick(c)}
    >
      <Server size={14} class={c.kind === "server" ? "term-ssh-server-icon" : "term-ssh-config-icon"} />
      <span class="term-ssh-name">{c.title}</span>
      <Show when={c.title !== c.detail}>
        <span class="term-ssh-detail muted">{c.detail}</span>
      </Show>
      <Show when={c.config?.hasSecret}>
        <KeyRound size={11} class="muted" />
      </Show>
    </div>
  );
  return (
    <div class="popup-backdrop" onMouseDown={(e) => e.target === e.currentTarget && props.request.pick(null)}>
      <div class="popup term-ssh-chooser" role="dialog" aria-label="Start SSH Session" tabIndex={-1} ref={popup} onKeyDown={onKeyDown}>
        <div class="term-ssh-title">Start SSH Session</div>
        <div class="term-ssh-list" role="listbox">
          <Show when={configs().length}>
            <div class="term-ssh-group muted">SSH Configurations</div>
            <For each={configs()}>{(c, i) => row(c, i)}</For>
          </Show>
          <Show when={servers().length}>
            <div class="term-ssh-group muted">Deployment Servers</div>
            <For each={servers()}>{(c, i) => row(c, () => configs().length + i())}</For>
          </Show>
          <Show when={!props.request.choices.length}>
            <div class="term-ssh-empty muted">No SSH configurations or SFTP deployment servers yet.</div>
          </Show>
        </div>
        <div class="term-ssh-footer">
          <div
            class="term-ssh-item"
            classList={{ selected: index() === props.request.choices.length }}
            role="option"
            data-ssh-choice="credentials"
            onMouseEnter={() => setIndex(props.request.choices.length)}
            onClick={() => props.request.pick("credentials")}
          >
            <Pencil size={13} />
            <span class="term-ssh-name">Edit credentials…</span>
          </div>
        </div>
      </div>
    </div>
  );
}

// ---- Edit credentials -------------------------------------------------------------------------

interface CredentialsRequest {
  config: SshConfiguration;
  hasSecret?: boolean;
  /** Why it opened (a configuration whose password is not saved). */
  message?: string;
  done: (r: { config: SshConfigView; secret?: string } | null) => void;
}

const [credentials, setCredentials] = createSignal<CredentialsRequest | null>(null);

function CredentialsDialog(props: { request: CredentialsRequest }) {
  const [config, setConfig] = createStore<SshConfiguration>(structuredClone(props.request.config));
  const [secret, setSecret] = createSignal("");
  const [problems, setProblems] = createSignal<string[]>([]);
  const [error, setError] = createSignal<string>();
  const [busy, setBusy] = createSignal(false);
  createEffect(
    on(
      () => JSON.stringify(config),
      () => void remoteApi.validate(structuredClone({ ...config })).then(setProblems, () => setProblems([])),
    ),
  );
  const ok = async () => {
    setBusy(true);
    setError(undefined);
    try {
      // The credentials become (or update) an SSH configuration, like PHPStorm's.
      const saved = await remoteApi.saveConfig({ ...config }, config.dontSaveSecret ? undefined : secret() || undefined);
      props.request.done({ config: saved, secret: secret() || undefined });
    } catch (e) {
      setError(String((e as Error)?.message ?? e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <div class="ssh-overlay" onMouseDown={(e) => e.target === e.currentTarget && props.request.done(null)}>
      <div
        class="ssh-dialog term-ssh-credentials"
        role="dialog"
        aria-label="SSH Session"
        data-testid="ssh-credentials"
        onKeyDown={(e) => e.key === "Escape" && props.request.done(null)}
      >
        <div class="ssh-titlebar">
          <span>SSH Session</span>
          <button class="icon-btn" title="Close" onClick={() => props.request.done(null)}>
            <X size={15} />
          </button>
        </div>
        <div class="ssh-body">
          <Show when={props.request.message}>
            <p class="ssh-small muted">{props.request.message}</p>
          </Show>
          <SshForm
            config={config}
            update={(patch) => setConfig(patch)}
            secret={secret()}
            setSecret={setSecret}
            hasSecret={props.request.hasSecret}
            problems={problems()}
            autofocus
          />
        </div>
        <div class="ssh-footer">
          <span class="ssh-error-text ssh-small">{error()}</span>
          <button class="btn-secondary" onClick={() => props.request.done(null)}>
            Cancel
          </button>
          <button class="btn-primary" disabled={busy() || problems().length > 0} onClick={() => void ok()} data-testid="ssh-credentials-ok">
            Connect
          </button>
        </div>
      </div>
    </div>
  );
}

function editCredentials(config: SshConfiguration, hasSecret?: boolean, message?: string) {
  return new Promise<{ config: SshConfigView; secret?: string } | null>((resolve) =>
    setCredentials({
      config,
      hasSecret,
      message,
      done: (r) => {
        setCredentials(null);
        resolve(r);
      },
    }),
  );
}

// ---- Starting a session -------------------------------------------------------------------------

let openTab: (title: string, ssh: SshTarget) => void = () => undefined;
/** The terminal feature opens the tab (avoids an import cycle). */
export function setSshTabOpener(open: (title: string, ssh: SshTarget) => void): void {
  openTab = open;
}

const needsSecret = (c: SshConfigView) => (c.auth === "password" && (!c.hasSecret || c.dontSaveSecret)) || (c.auth === "keyPair" && c.dontSaveSecret);

/** Start an SSH session to a chosen entry: confirm in Safe Mode, ask for a password the configuration does not save. */
export async function startSession(choice: SshChoice): Promise<void> {
  let config = choice.config;
  if (!config) {
    showBalloon("Cannot start the SSH session", `The SSH configuration ${choice.detail} of ${choice.title} does not exist anymore.`);
    return;
  }
  const target: SshTarget = choice.kind === "server" ? { serverId: choice.id } : { configId: choice.id };
  if (choice.kind === "server" && safeMode()) {
    const ok = await askConfirm({
      title: "Start SSH Session",
      message: `The deployment server ${choice.title} is defined in the files of this untrusted project. Connect to ${config.label} over SSH with your credentials?`,
      confirm: "Connect",
    });
    if (!ok) return;
    target.confirmed = true;
  }
  if (needsSecret(config)) {
    const r = await editCredentials(config, config.hasSecret, `Enter the ${config.auth === "keyPair" ? "passphrase" : "password"} for ${config.label}.`);
    if (!r) return;
    config = r.config;
    target.secret = r.secret;
  }
  openTab(sshTabTitle(config.host), target);
}

/** Tools | Start SSH Session…: the chooser, then a new Terminal tab. */
export async function chooseSshSession(): Promise<void> {
  const [configs, deployment] = await Promise.all([remoteApi.configs().catch(() => []), deploymentApi.config().catch(() => null)]);
  const choices = sshChoices(configs, deployment?.servers ?? []);
  const picked = await new Promise<SshChoice | "credentials" | null>((resolve) =>
    setChooser({
      choices,
      pick: (c) => {
        setChooser(null);
        resolve(c);
      },
    }),
  );
  if (!picked) return;
  if (picked !== "credentials") return startSession(picked);
  const r = await editCredentials(blankConfig());
  if (r) openTab(sshTabTitle(r.config.host), { configId: r.config.id, secret: r.secret });
}

/**
 * The host key of the target is unknown or changed: ask (Test Connection's question) and
 * remember it in `~/.ssh/known_hosts`. True when the user trusted it.
 */
export async function confirmSshHostKey(target: SshTarget): Promise<boolean> {
  const configs = await remoteApi.configs().catch(() => []);
  let id = target.configId;
  if (target.serverId) {
    const servers = (await deploymentApi.config().catch(() => null))?.servers ?? [];
    id = servers.find((s) => s.id === target.serverId)?.sshConfig;
  }
  const config = configs.find((c) => c.id === id);
  if (!config) return false;
  const { label: _l, hasSecret: _h, ...plain } = config;
  const r = await testConnection(plain, target.secret);
  return r.ok;
}

if (typeof window !== "undefined" && !isDialogWindow()) {
  const host = document.createElement("div");
  host.id = "ssh-session-overlays";
  document.body.append(host);
  render(
    () => (
      <>
        <Show when={chooser()}>{(r) => <Chooser request={r()} />}</Show>
        <Show when={credentials()}>{(r) => <CredentialsDialog request={r()} />}</Show>
      </>
    ),
    host,
  );
}
