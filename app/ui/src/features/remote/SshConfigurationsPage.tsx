/**
 * Settings | Tools | SSH Configurations: saved SSH connections (host, port, user,
 * authentication). Passwords and passphrases are kept in the system keychain, never in
 * settings files.
 */
import { createEffect, createResource, createSignal, For, on, Show } from "solid-js";
import { createStore, reconcile } from "solid-js/store";
import { Copy, KeyRound, Minus, Plus, Server } from "lucide-solid";
import { remoteApi, type SshConfiguration, type SshConfigView } from "@/ipc/remote";
import { AUTH_LABELS, SshForm } from "./SshForm";
import { TestResultLine, useConnectionTest } from "./TestConnection";
import { blankConfig } from "./store";
import { t } from "@/i18n";

const NEW = "__new__";

export function SshConfigurationsPage() {
  const [list, { refetch }] = createResource(() => remoteApi.configs());
  const [selected, setSelected] = createSignal<string | null>(null);
  const [draft, setDraft] = createStore<SshConfiguration>(blankConfig());
  const [secret, setSecret] = createSignal("");
  const [dirty, setDirty] = createSignal(false);
  const [problems, setProblems] = createSignal<string[]>([]);
  const [saveError, setSaveError] = createSignal<string>();
  const test = useConnectionTest();

  const current = () => list()?.find((c) => c.id === selected());

  createEffect(() => {
    const l = list();
    if (selected() === null && l?.length) select(l[0]);
  });

  createEffect(
    on(
      () => JSON.stringify(draft),
      () => {
        if (!selected()) return;
        void remoteApi.validate({ ...draft }).then(setProblems).catch(() => setProblems([]));
      },
    ),
  );

  function select(c: SshConfigView | null) {
    setSelected(c ? c.id : NEW);
    const { label: _l, hasSecret: _h, ...config } = c ?? { ...blankConfig(), label: "", hasSecret: false };
    setDraft(reconcile(structuredClone(config)));
    setSecret("");
    setDirty(false);
    setSaveError(undefined);
    test.reset();
  }

  const update = (patch: Partial<SshConfiguration>) => {
    setDraft(patch);
    setDirty(true);
    test.reset();
  };

  const save = async () => {
    setSaveError(undefined);
    try {
      const saved = await remoteApi.saveConfig({ ...draft }, secret() || undefined);
      await refetch();
      select(saved);
    } catch (e) {
      setSaveError(String((e as Error)?.message ?? e));
    }
  };

  const remove = async () => {
    const c = current();
    if (c) await remoteApi.deleteConfig(c.id);
    setSelected(null);
    await refetch();
    if (!list()?.length) select(null);
  };

  const duplicate = () => {
    const c = current();
    if (!c) return;
    select(null);
    const { label: _l, hasSecret: _h, ...config } = c;
    setDraft(reconcile({ ...structuredClone(config), id: "", name: c.name ? `${c.name} (copy)` : undefined }));
    setDirty(true);
  };

  return (
    <div class="ssh-settings">
      <p class="ssh-settings-intro muted">
        {t("tools.remote.sshConnectionsForRemoteDevelopment")}
        <code>~/.ssh/config</code> {t("tools.remote.hostsCanBeUsedBy")}
      </p>
      <div class="ssh-master-detail">
        <div class="ssh-list-pane">
          <div class="ssh-list-toolbar">
            <button class="icon-btn" title={t("common.add")} onClick={() => select(null)} data-testid="ssh-config-add">
              <Plus size={16} />
            </button>
            <button class="icon-btn" title={t("common.remove")} disabled={!current()} onClick={() => void remove()}>
              <Minus size={16} />
            </button>
            <button class="icon-btn" title={t("common.copy")} disabled={!current()} onClick={duplicate}>
              <Copy size={14} />
            </button>
          </div>
          <div class="ssh-list">
            <For each={list() ?? []}>
              {(c) => (
                <button
                  class="ssh-list-row"
                  classList={{ selected: selected() === c.id }}
                  onClick={() => select(c)}
                  data-config={c.id}
                >
                  <Server size={15} class="ssh-folder-icon" />
                  <span class="ssh-list-text">
                    <span class="ssh-list-title">{c.name || c.label}</span>
                    <span class="ssh-list-sub muted">
                      {AUTH_LABELS[c.auth]}
                      <Show when={c.hasSecret}>
                        {" "}
                        <KeyRound size={11} />
                      </Show>
                    </span>
                  </span>
                </button>
              )}
            </For>
            <Show when={selected() === NEW}>
              <div class="ssh-list-row selected">
                <Server size={15} class="ssh-folder-icon" />
                <span class="ssh-list-text">
                  <span class="ssh-list-title">{draft.host ? `${draft.user || "?"}@${draft.host}` : t("tools.remote.newConfiguration")}</span>
                  <span class="ssh-list-sub muted">{t("common.notSavedYet")}</span>
                </span>
              </div>
            </Show>
            <Show when={!list.loading && !list()?.length && selected() !== NEW}>
              <div class="ssh-empty muted">{t("tools.remote.noSshConfigurations")}</div>
            </Show>
          </div>
        </div>
        <div class="ssh-detail-pane">
          <Show when={selected()} fallback={<div class="ssh-empty muted">{t("tools.remote.addAnSshConfigurationTo")}</div>}>
            <div class="ssh-name-row">
              <label class="ssh-label" for="ssh-config-name">
                {t("common.nameLabel")}
              </label>
              <input
                id="ssh-config-name"
                class="ssh-input"
                placeholder={draft.host ? `${draft.user || "user"}@${draft.host}` : t("tools.remote.optional")}
                value={draft.name ?? ""}
                onInput={(e) => update({ name: e.currentTarget.value || undefined })}
              />
            </div>
            <SshForm
              config={draft}
              update={update}
              secret={secret()}
              setSecret={(s) => {
                setSecret(s);
                setDirty(true);
                test.reset();
              }}
              hasSecret={current()?.hasSecret}
              problems={dirty() ? problems() : []}
            />
            <div class="ssh-test-row">
              <button
                class="btn-secondary"
                disabled={problems().length > 0 || test.state()?.running === true}
                onClick={() => void test.run({ ...draft }, secret())}
              >
                {t("common.testConnection")}
              </button>
              <TestResultLine state={test.state()} host={draft.host.trim()} />
            </div>
            <div class="ssh-detail-actions">
              <Show when={saveError()}>
                <span class="ssh-error-text">{saveError()}</span>
              </Show>
              <button class="btn-secondary" disabled={!dirty()} onClick={() => select(current() ?? null)}>
                {t("common.reset")}
              </button>
              <button class="btn-primary" disabled={!dirty() || problems().length > 0} onClick={() => void save()}>
                {t("common.save")}
              </button>
            </div>
          </Show>
        </div>
      </div>
    </div>
  );
}
