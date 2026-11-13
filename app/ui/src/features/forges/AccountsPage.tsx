/**
 * Settings | Version Control | GitLab / Forgejo / Gitea / GitHub / Azure DevOps: accounts per host. Tokens
 * are verified against the server and stored in the OS keychain (service `forge-forges`).
 */
import { createResource, createSignal, For, Show } from "solid-js";
import { createStore } from "solid-js/store";
import { CircleCheck, CircleX, ExternalLink, KeyRound, LoaderCircle, Minus, Plug, Plus } from "lucide-solid";
import { defaultServers, forgesApi, hostNames, type Account, type HostKind } from "@/ipc/forges";
import { Avatar } from "./ui";
import { errorText, refreshAll } from "./store";
import { t } from "@/i18n";

type Check = { running: true } | { running: false; ok: boolean; text: string };

function tokenUrl(kind: HostKind, server: string): string {
  const s = server.replace(/\/$/, "") || defaultServers[kind];
  switch (kind) {
    case "gitlab":
      return `${s}/-/user_settings/personal_access_tokens?name=Forge&scopes=api,read_user`;
    case "github":
      return `${s === "https://github.com" ? s : `${s}`}/settings/tokens/new?description=Forge&scopes=repo,read:org,workflow`;
    case "azure":
      // The organization's personal access tokens page.
      return `${s}/_usersSettings/tokens`;
    default:
      return `${s}/user/settings/applications`;
  }
}

const scopes: Record<HostKind, string> = {
  gitlab: "Scopes: api, read_user",
  github: "Scopes: repo, read:org, workflow (or a fine-grained token with pull request, issue and actions access)",
  forgejo: "Permissions: read/write for repository and issue, read for user",
  azure: "Scopes: Code (Read & Write), Work Items (Read & Write), Build (Read), Project and Team (Read)",
};

const intro: Record<HostKind, string> = {
  gitlab: "Review merge requests, issues and pipelines on gitlab.com and self-managed GitLab servers.",
  forgejo: "Review pull requests, issues and Actions runs on Forgejo and Gitea servers such as Codeberg.",
  github: "Review pull requests, issues and workflow runs on github.com and GitHub Enterprise Server.",
  azure: "Review pull requests, work items and builds on Azure DevOps Services (https://dev.azure.com/<organization>) and Azure DevOps Server (the collection URL).",
};

export function AccountsPage(props: { kind: HostKind }) {
  const [accounts, { refetch }] = createResource(() => forgesApi.accounts());
  const mine = () => (accounts() ?? []).filter((a) => a.kind === props.kind);
  const [selected, setSelected] = createSignal<string | null>(null);
  const [checks, setChecks] = createStore<Record<string, Check>>({});
  const [form, setForm] = createSignal(false);
  const [server, setServer] = createSignal(defaultServers[props.kind]);
  const [token, setToken] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);

  const openForm = () => {
    setServer(defaultServers[props.kind]);
    setToken("");
    setError(null);
    setForm(true);
  };

  const login = async () => {
    setBusy(true);
    setError(null);
    try {
      const acc = await forgesApi.login(props.kind, server(), token());
      setForm(false);
      setToken("");
      await refetch();
      setSelected(acc.id);
      setChecks(acc.id, { running: false, ok: true, text: `Logged in as ${acc.username}` });
      void refreshAll();
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  const verify = async (a: Account) => {
    setChecks(a.id, { running: true });
    try {
      const u = await forgesApi.verify(a.id);
      setChecks(a.id, { running: false, ok: true, text: `Token is valid (${u.username})` });
    } catch (e) {
      setChecks(a.id, { running: false, ok: false, text: errorText(e) });
    }
  };

  const remove = async () => {
    const id = selected();
    if (!id) return;
    await forgesApi.logout(id);
    setSelected(null);
    await refetch();
    void refreshAll();
  };

  return (
    <div class="fg-settings" data-hosts-page={props.kind}>
      <p class="muted fg-settings-intro">{intro[props.kind]}</p>
      <div class="fg-accounts">
        <div class="fg-accounts-toolbar">
          <button class="icon-btn" title={t("tools.forges.logInWithTokenEllipsis")} onClick={openForm} data-action="add-account">
            <Plus size={16} />
          </button>
          <button class="icon-btn" title={t("tools.forges.removeAccount")} disabled={!selected()} onClick={() => void remove()}>
            <Minus size={16} />
          </button>
        </div>
        <div class="fg-accounts-list">
          <For
            each={mine()}
            fallback={
              <div class="fg-accounts-empty muted">
                {t("tools.forges.noAccounts", { value: hostNames[props.kind] })}
                <button class="btn-secondary" onClick={openForm}>
                  <KeyRound size={14} />
                  {t("tools.forges.logInWithTokenEllipsis")}
                </button>
              </div>
            }
          >
            {(a) => {
              const check = () => checks[a.id];
              return (
                <div class="fg-account" classList={{ selected: selected() === a.id }} onClick={() => setSelected(a.id)} data-account={a.id}>
                  <Avatar user={{ username: a.username ?? "?", name: null, avatarUrl: null }} size={28} />
                  <div class="fg-account-text">
                    <div class="fg-account-name">{a.username ?? t("tools.forges.unknownUser")}</div>
                    <div class="muted fg-small">{a.server.replace(/^https?:\/\//, "")}</div>
                  </div>
                  <span class="fg-token-source" classList={{ missing: a.tokenSource === "none" }}>
                    {a.tokenSource === "keychain" ? t("tools.forges.tokenInKeychain") : a.tokenSource === "env" ? t("tools.forges.tokenFromEnvironment") : t("tools.forges.noToken")}
                  </span>
                  <Show when={check()}>
                    {(c) => (
                      <span class="fg-check" classList={{ ok: !c().running && (c() as { ok: boolean }).ok, bad: !c().running && !(c() as { ok: boolean }).ok }} title={c().running ? undefined : (c() as { text: string }).text}>
                        <Show when={!c().running} fallback={<LoaderCircle size={14} class="spin" />}>
                          <Show when={(c() as { ok: boolean }).ok} fallback={<CircleX size={14} />}>
                            <CircleCheck size={14} />
                          </Show>
                        </Show>
                        <span class="fg-check-text">{c().running ? t("tools.forges.checkingEllipsis") : (c() as { text: string }).text}</span>
                      </span>
                    )}
                  </Show>
                  <button
                    class="btn-secondary fg-small-btn"
                    onClick={(e) => {
                      e.stopPropagation();
                      void verify(a);
                    }}
                  >
                    <Plug size={13} />
                    {t("common.test")}
                  </button>
                </div>
              );
            }}
          </For>
        </div>
      </div>

      <Show when={form()}>
        <div class="fg-login" data-login-form>
          <div class="fg-login-title">
            <KeyRound size={15} />
            {t("tools.forges.logInWithToken")}
          </div>
          <div class="fg-login-grid">
            <label for="fg-server">{t("tools.forges.serverLabel")}</label>
            <input id="fg-server" class="mono" value={server()} spellcheck={false} onInput={(e) => setServer(e.currentTarget.value)} />
            <label for="fg-token">{t("tools.forges.tokenLabel")}</label>
            <div>
              <input
                id="fg-token"
                type="password"
                autocomplete="off"
                placeholder={props.kind === "gitlab" ? t("tools.forges.glpatEllipsis") : props.kind === "github" ? t("tools.forges.ghpOrGithubPatEllipsis") : props.kind === "azure" ? t("tools.forges.personalAccessToken") : t("tools.forges.accessToken")}
                value={token()}
                onInput={(e) => setToken(e.currentTarget.value)}
                onKeyDown={(e) => e.key === "Enter" && token().trim() && void login()}
              />
              <div class="muted fg-small fg-login-hint">
                {scopes[props.kind]} ·{" "}
                <a class="fg-link" href={tokenUrl(props.kind, server())} target="_blank" rel="noreferrer">
                  {t("tools.forges.generateEllipsis")}
                  <ExternalLink size={11} />
                </a>
              </div>
            </div>
          </div>
          <Show when={error()}>
            <div class="fg-error-box">
              <CircleX size={14} />
              {error()}
            </div>
          </Show>
          <div class="fg-login-actions">
            <span class="muted fg-small">{t("tools.forges.theTokenIsVerifiedAnd")}</span>
            <span class="spacer" />
            <button class="btn-secondary" onClick={() => setForm(false)}>
              {t("common.cancel")}
            </button>
            <button class="btn-primary" disabled={busy() || !token().trim()} onClick={() => void login()} data-action="login">
              <Show when={busy()}>
                <LoaderCircle size={13} class="spin" />
              </Show>
              {t("tools.forges.logIn")}
            </button>
          </div>
        </div>
      </Show>
    </div>
  );
}
