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
        SSH connections for remote development. Passwords and key passphrases are stored in the system keychain;{" "}
        <code>~/.ssh/config</code> hosts can be used by their alias.
      </p>
      <div class="ssh-master-detail">
        <div class="ssh-list-pane">
          <div class="ssh-list-toolbar">
            <button class="icon-btn" title="Add" onClick={() => select(null)} data-testid="ssh-config-add">
              <Plus size={16} />
            </button>
            <button class="icon-btn" title="Remove" disabled={!current()} onClick={() => void remove()}>
              <Minus size={16} />
            </button>
            <button class="icon-btn" title="Copy" disabled={!current()} onClick={duplicate}>
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
                  <span class="ssh-list-title">{draft.host ? `${draft.user || "?"}@${draft.host}` : "New configuration"}</span>
                  <span class="ssh-list-sub muted">Not saved yet</span>
                </span>
              </div>
            </Show>
            <Show when={!list.loading && !list()?.length && selected() !== NEW}>
              <div class="ssh-empty muted">No SSH configurations.</div>
            </Show>
          </div>
        </div>
        <div class="ssh-detail-pane">
          <Show when={selected()} fallback={<div class="ssh-empty muted">Add an SSH configuration to connect to a remote host.</div>}>
            <div class="ssh-name-row">
              <label class="ssh-label" for="ssh-config-name">
                Name:
              </label>
              <input
                id="ssh-config-name"
                class="ssh-input"
                placeholder={draft.host ? `${draft.user || "user"}@${draft.host}` : "Optional"}
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
                Test Connection
              </button>
              <TestResultLine state={test.state()} host={draft.host.trim()} />
            </div>
            <div class="ssh-detail-actions">
              <Show when={saveError()}>
                <span class="ssh-error-text">{saveError()}</span>
              </Show>
              <button class="btn-secondary" disabled={!dirty()} onClick={() => select(current() ?? null)}>
                Reset
              </button>
              <button class="btn-primary" disabled={!dirty() || problems().length > 0} onClick={() => void save()}>
                Save
              </button>
            </div>
          </Show>
        </div>
      </div>
    </div>
  );
}
