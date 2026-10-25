/**
 * Settings | Build, Execution, Deployment | Deployment: the project's servers (SFTP, FTP,
 * FTPS, local or mounted folder) with the Connection, Mappings and Excluded Paths tabs, Test
 * Connection and "Use as Default", like PHPStorm. Servers are saved to
 * `.forge/deployment.toml`; FTP passwords go to the system keychain.
 */
import { createEffect, createResource, createSignal, For, Match, on, Show, Switch } from "solid-js";
import { createStore, reconcile } from "solid-js/store";
import { CircleCheck, CircleX, Copy, Folder, FolderInput, Globe, KeyRound, LoaderCircle, Minus, Plus, Server, Star } from "lucide-solid";
import { blankServer, deploymentApi, type DeployServer, type ServerKind, type ServerView, type TestResult } from "@/ipc/deployment";
import { remoteApi } from "@/ipc/remote";
import { showContextMenu, item } from "@/features/contextMenu/state";
import { openSettings } from "@/features/settings/dialogState";
import { refreshConfig } from "./store";
import "@/features/remote/remote.css";
import "./deployment.css";

const NEW = "__new__";

/** A plain copy of the form (store proxies cannot be cloned or sent as they are). */
const plain = (s: DeployServer): DeployServer => JSON.parse(JSON.stringify(s)) as DeployServer;

export const KIND_LABELS: Record<ServerKind, string> = {
  sftp: "SFTP",
  ftp: "FTP",
  ftps: "FTPS",
  local: "Local or mounted folder",
};

export function KindIcon(props: { kind: ServerKind; size?: number }) {
  return (
    <Switch fallback={<Server size={props.size ?? 15} class="deploy-kind-icon" />}>
      <Match when={props.kind === "local"}>
        <Folder size={props.size ?? 15} class="deploy-kind-icon local" />
      </Match>
      <Match when={props.kind === "ftp" || props.kind === "ftps"}>
        <Globe size={props.size ?? 15} class="deploy-kind-icon ftp" />
      </Match>
    </Switch>
  );
}

type Tab = "connection" | "mappings" | "excluded";

export function DeploymentPage() {
  const [data, { refetch }] = createResource(() => deploymentApi.config());
  const [sshConfigs] = createResource(() => remoteApi.configs().catch(() => []));
  const [selected, setSelected] = createSignal<string | null>(null);
  const [draft, setDraft] = createStore<DeployServer>(blankServer());
  const [secret, setSecret] = createSignal("");
  const [dirty, setDirty] = createSignal(false);
  const [problems, setProblems] = createSignal<string[]>([]);
  const [saveError, setSaveError] = createSignal<string>();
  const [tab, setTab] = createSignal<Tab>("connection");
  const [test, setTest] = createSignal<({ running: true } | ({ running: false } & TestResult)) | null>(null);

  const servers = () => data()?.servers ?? [];
  const current = () => servers().find((s) => s.id === selected());
  const isDefault = (id: string) => data()?.defaultServer === id;

  createEffect(() => {
    const list = servers();
    if (selected() === null && list.length) select(list.find((s) => isDefault(s.id)) ?? list[0]);
  });

  createEffect(
    on(
      () => JSON.stringify(draft),
      () => {
        if (!selected()) return;
        void deploymentApi.validate(plain(draft)).then(setProblems, () => setProblems([]));
      },
    ),
  );

  function select(s: ServerView | null, kind: ServerKind = "sftp") {
    setSelected(s ? s.id : NEW);
    const { label: _l, hasSecret: _h, ...server } = s ?? { ...blankServer(kind), label: "", hasSecret: false };
    setDraft(reconcile(structuredClone(server)));
    setSecret("");
    setDirty(false);
    setSaveError(undefined);
    setTest(null);
    setTab("connection");
  }

  const update = (patch: Partial<DeployServer>) => {
    setDraft(patch);
    setDirty(true);
    setTest(null);
  };

  const addMenu = (ev: MouseEvent) => {
    const r = (ev.currentTarget as HTMLElement).getBoundingClientRect();
    showContextMenu(
      (Object.keys(KIND_LABELS) as ServerKind[]).map((k) => item(KIND_LABELS[k], () => select(null, k))),
      r.left,
      r.bottom + 2,
    );
  };

  const save = async () => {
    setSaveError(undefined);
    try {
      const saved = await deploymentApi.saveServer(plain(draft), secret() || undefined);
      await refetch();
      void refreshConfig();
      select(servers().find((s) => s.id === saved.id) ?? saved);
    } catch (e) {
      setSaveError(String((e as Error)?.message ?? e));
    }
  };

  const remove = async () => {
    const c = current();
    if (c) await deploymentApi.deleteServer(c.id);
    setSelected(null);
    await refetch();
    void refreshConfig();
    if (!servers().length) setSelected(null);
  };

  const duplicate = () => {
    const c = current();
    if (!c) return;
    const { label: _l, hasSecret: _h, ...server } = c;
    setSelected(NEW);
    setDraft(reconcile({ ...structuredClone(server), id: "", name: `${c.name} (copy)` }));
    setDirty(true);
    setTab("connection");
  };

  const useAsDefault = async () => {
    const c = current();
    if (!c) return;
    await deploymentApi.setDefault(isDefault(c.id) ? null : c.id);
    await refetch();
    void refreshConfig();
  };

  const runTest = async () => {
    setTest({ running: true });
    try {
      setTest({ running: false, ...(await deploymentApi.test(plain(draft), secret() || undefined)) });
    } catch (e) {
      setTest({ running: false, ok: false, message: String((e as Error)?.message ?? e) });
    }
  };

  /** "Autodetect": the login folder the server reports. */
  const autodetect = async () => {
    setTest({ running: true });
    const result = await deploymentApi.test({ ...plain(draft), rootPath: "" }, secret() || undefined).catch(
      (e): TestResult => ({ ok: false, message: String((e as Error)?.message ?? e) }),
    );
    setTest({ running: false, ...result });
    if (result.root) update({ rootPath: result.root });
  };

  return (
    <div class="ssh-settings deploy-settings">
      <p class="ssh-settings-intro muted">
        Servers to deploy the project to. Passwords are stored in the system keychain; SFTP servers use an SSH configuration.
      </p>
      <div class="ssh-master-detail deploy-master-detail">
        <div class="ssh-list-pane">
          <div class="ssh-list-toolbar">
            <button class="icon-btn" title="Add" onClick={addMenu} data-testid="deploy-add">
              <Plus size={16} />
            </button>
            <button class="icon-btn" title="Remove" disabled={!current()} onClick={() => void remove()}>
              <Minus size={16} />
            </button>
            <button class="icon-btn" title="Copy" disabled={!current()} onClick={duplicate}>
              <Copy size={14} />
            </button>
            <button
              class="icon-btn"
              classList={{ active: !!current() && isDefault(current()!.id) }}
              title="Use as Default"
              aria-pressed={!!current() && isDefault(current()!.id)}
              disabled={!current()}
              onClick={() => void useAsDefault()}
            >
              <Star size={14} />
            </button>
          </div>
          <div class="ssh-list">
            <For each={servers()}>
              {(s) => (
                <button class="ssh-list-row" classList={{ selected: selected() === s.id }} onClick={() => select(s)} data-server={s.id}>
                  <KindIcon kind={s.kind} />
                  <span class="ssh-list-text">
                    <span class="ssh-list-title" classList={{ "deploy-default": isDefault(s.id) }}>
                      {s.name || s.label}
                    </span>
                    <span class="ssh-list-sub muted">
                      {KIND_LABELS[s.kind]}
                      <Show when={isDefault(s.id)}> · default</Show>
                      <Show when={s.hasSecret}>
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
                <KindIcon kind={draft.kind} />
                <span class="ssh-list-text">
                  <span class="ssh-list-title">{draft.name || "New server"}</span>
                  <span class="ssh-list-sub muted">Not saved yet</span>
                </span>
              </div>
            </Show>
            <Show when={!data.loading && !servers().length && selected() !== NEW}>
              <div class="ssh-empty muted">No deployment servers.</div>
            </Show>
          </div>
        </div>
        <div class="ssh-detail-pane">
          <Show when={selected()} fallback={<div class="ssh-empty muted">Add a server (+) to deploy the project with SFTP, FTP or to a local folder.</div>}>
            <div class="ssh-name-row">
              <label class="ssh-label" for="deploy-name">
                Name:
              </label>
              <input id="deploy-name" class="ssh-input" value={draft.name} placeholder="e.g. production" onInput={(e) => update({ name: e.currentTarget.value })} />
            </div>
            <div class="deploy-tabs" role="tablist">
              <For each={[["connection", "Connection"], ["mappings", "Mappings"], ["excluded", "Excluded Paths"]] as [Tab, string][]}>
                {([id, title]) => (
                  <button class="deploy-tab" role="tab" classList={{ active: tab() === id }} aria-selected={tab() === id} onClick={() => setTab(id)}>
                    {title}
                  </button>
                )}
              </For>
            </div>
            <Switch>
              <Match when={tab() === "connection"}>
                <div class="ssh-form">
                  <label class="ssh-label" for="deploy-kind">
                    Type:
                  </label>
                  <select id="deploy-kind" class="ssh-input" value={draft.kind} onChange={(e) => update({ kind: e.currentTarget.value as ServerKind })}>
                    <For each={Object.keys(KIND_LABELS) as ServerKind[]}>
                      {(k) => (
                        <option value={k} selected={k === draft.kind}>
                          {KIND_LABELS[k]}
                        </option>
                      )}
                    </For>
                  </select>
                  <Switch>
                    <Match when={draft.kind === "sftp"}>
                      <label class="ssh-label" for="deploy-ssh">
                        SSH configuration:
                      </label>
                      <div class="ssh-path">
                        <select id="deploy-ssh" class="ssh-input" value={draft.sshConfig} onChange={(e) => update({ sshConfig: e.currentTarget.value })}>
                          <option value="">Select an SSH configuration</option>
                          <For each={sshConfigs() ?? []}>
                            {(c) => (
                              <option value={c.id} selected={c.id === draft.sshConfig}>
                                {c.name || c.label}
                              </option>
                            )}
                          </For>
                        </select>
                        <button class="ssh-browse" title="SSH Configurations…" onClick={() => openSettings("tools.ssh")}>
                          …
                        </button>
                      </div>
                    </Match>
                    <Match when={draft.kind === "ftp" || draft.kind === "ftps"}>
                      <label class="ssh-label" for="deploy-host">
                        Host:
                      </label>
                      <div class="ssh-host-row">
                        <input id="deploy-host" class="ssh-input" value={draft.host} placeholder="ftp.example.com" spellcheck={false} onInput={(e) => update({ host: e.currentTarget.value })} />
                        <label class="ssh-label inline" for="deploy-port">
                          Port:
                        </label>
                        <input
                          id="deploy-port"
                          class="ssh-input ssh-port"
                          inputMode="numeric"
                          value={draft.port ?? 21}
                          onInput={(e) => {
                            const n = Number.parseInt(e.currentTarget.value, 10);
                            update({ port: Number.isFinite(n) && n !== 21 ? n : null });
                          }}
                        />
                      </div>
                      <label class="ssh-label" for="deploy-user">
                        Username:
                      </label>
                      <div class="ssh-secret-row">
                        <input id="deploy-user" class="ssh-input" value={draft.user} disabled={draft.anonymous} spellcheck={false} onInput={(e) => update({ user: e.currentTarget.value })} />
                        <label class="ssh-check">
                          <input type="checkbox" checked={draft.anonymous} onChange={(e) => update({ anonymous: e.currentTarget.checked })} /> Anonymous login
                        </label>
                      </div>
                      <label class="ssh-label" for="deploy-password">
                        Password:
                      </label>
                      <input
                        id="deploy-password"
                        class="ssh-input"
                        type="password"
                        disabled={draft.anonymous}
                        placeholder={current()?.hasSecret ? "Stored in the keychain" : ""}
                        value={secret()}
                        onInput={(e) => {
                          setSecret(e.currentTarget.value);
                          setDirty(true);
                          setTest(null);
                        }}
                      />
                      <span />
                      <label class="ssh-check">
                        <input type="checkbox" checked={draft.passive} onChange={(e) => update({ passive: e.currentTarget.checked })} /> Passive mode
                      </label>
                    </Match>
                  </Switch>
                  <label class="ssh-label" for="deploy-root">
                    {draft.kind === "local" ? "Folder:" : "Root path:"}
                  </label>
                  <div class="ssh-path">
                    <input
                      id="deploy-root"
                      class="ssh-input mono"
                      value={draft.rootPath}
                      placeholder={draft.kind === "local" ? "/srv/http/shop" : "Empty: the login folder"}
                      spellcheck={false}
                      onInput={(e) => update({ rootPath: e.currentTarget.value })}
                    />
                    <Show when={draft.kind !== "local"}>
                      <button class="btn-secondary deploy-autodetect" disabled={problems().length > 0 || test()?.running === true} onClick={() => void autodetect()}>
                        Autodetect
                      </button>
                    </Show>
                  </div>
                  <label class="ssh-label" for="deploy-url">
                    Web server URL:
                  </label>
                  <input id="deploy-url" class="ssh-input mono" value={draft.webUrl} placeholder="https://example.com" spellcheck={false} onInput={(e) => update({ webUrl: e.currentTarget.value })} />
                </div>
                <div class="ssh-test-row deploy-test-row">
                  <button class="btn-secondary" disabled={problems().length > 0 || test()?.running === true} onClick={() => void runTest()} data-testid="deploy-test">
                    Test Connection
                  </button>
                  <Show when={test()}>
                    {(t) => (
                      <div class="ssh-test-result">
                        <Switch>
                          <Match when={t().running}>
                            <LoaderCircle size={14} class="ssh-spin" />
                            <span class="muted">Connecting…</span>
                          </Match>
                          <Match when={!t().running && (t() as TestResult)}>
                            {(r) => (
                              <>
                                {r().ok ? <CircleCheck size={14} class="ssh-ok" /> : <CircleX size={14} class="ssh-bad" />}
                                <span class={r().ok ? "" : "ssh-error-text"}>
                                  {r().message}
                                  <Show when={r().ok && r().root}> ({r().root})</Show>
                                </span>
                              </>
                            )}
                          </Match>
                        </Switch>
                      </div>
                    )}
                  </Show>
                </div>
              </Match>
              <Match when={tab() === "mappings"}>
                <MappingsTab draft={draft} update={update} />
              </Match>
              <Match when={tab() === "excluded"}>
                <ExcludedTab draft={draft} update={update} />
              </Match>
            </Switch>
            <Show when={dirty() && problems().length}>
              <ul class="ssh-problems">
                <For each={problems()}>{(p) => <li>{p}</li>}</For>
              </ul>
            </Show>
            <div class="ssh-detail-actions">
              <Show when={saveError()}>
                <span class="ssh-error-text">{saveError()}</span>
              </Show>
              <button class="btn-secondary" disabled={!dirty()} onClick={() => select(current() ?? null, draft.kind)}>
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

function MappingsTab(props: { draft: DeployServer; update: (p: Partial<DeployServer>) => void }) {
  const [row, setRow] = createSignal(0);
  const set = (i: number, key: "local" | "deployment" | "web", value: string) =>
    props.update({ mappings: props.draft.mappings.map((m, j) => (j === i ? { ...m, [key]: value } : m)) });
  return (
    <div class="deploy-table-wrap">
      <div class="deploy-table-toolbar">
        <button class="icon-btn" title="Add Mapping" onClick={() => props.update({ mappings: [...props.draft.mappings, { local: "", deployment: "", web: "" }] })}>
          <Plus size={15} />
        </button>
        <button
          class="icon-btn"
          title="Remove Mapping"
          disabled={!props.draft.mappings.length}
          onClick={() => {
            props.update({ mappings: props.draft.mappings.filter((_, j) => j !== row()) });
            setRow(Math.max(0, row() - 1));
          }}
        >
          <Minus size={15} />
        </button>
      </div>
      <table class="deploy-table">
        <thead>
          <tr>
            <th>Local path</th>
            <th>Deployment path</th>
            <th>Web path</th>
          </tr>
        </thead>
        <tbody>
          <For each={props.draft.mappings}>
            {(m, i) => (
              <tr classList={{ selected: row() === i() }} onFocusIn={() => setRow(i())}>
                <td>
                  <div class="deploy-cell">
                    <FolderInput size={13} class="muted" />
                    <input class="deploy-cell-input mono" value={m.local} placeholder="(project root)" spellcheck={false} onInput={(e) => set(i(), "local", e.currentTarget.value)} />
                  </div>
                </td>
                <td>
                  <input class="deploy-cell-input mono" value={m.deployment} placeholder="/" spellcheck={false} onInput={(e) => set(i(), "deployment", e.currentTarget.value)} />
                </td>
                <td>
                  <input class="deploy-cell-input mono" value={m.web} placeholder="/" spellcheck={false} onInput={(e) => set(i(), "web", e.currentTarget.value)} />
                </td>
              </tr>
            )}
          </For>
        </tbody>
      </table>
      <Show when={!props.draft.mappings.length}>
        <p class="ssh-hint">No mappings: the project root is deployed to the root path.</p>
      </Show>
      <p class="ssh-hint">Deployment paths are relative to the root path; web paths to the web server URL. The longest matching local path wins.</p>
    </div>
  );
}

function ExcludedTab(props: { draft: DeployServer; update: (p: Partial<DeployServer>) => void }) {
  const [row, setRow] = createSignal(0);
  const add = (side: "local" | "deployment") => props.update({ excluded: [...props.draft.excluded, { side, path: "" }] });
  return (
    <div class="deploy-table-wrap">
      <div class="deploy-table-toolbar">
        <button class="btn-secondary deploy-small-btn" onClick={() => add("local")}>
          <Plus size={13} /> Add Local Path
        </button>
        <button class="btn-secondary deploy-small-btn" onClick={() => add("deployment")}>
          <Plus size={13} /> Add Deployment Path
        </button>
        <button
          class="icon-btn"
          title="Remove"
          disabled={!props.draft.excluded.length}
          onClick={() => {
            props.update({ excluded: props.draft.excluded.filter((_, j) => j !== row()) });
            setRow(Math.max(0, row() - 1));
          }}
        >
          <Minus size={15} />
        </button>
      </div>
      <table class="deploy-table">
        <thead>
          <tr>
            <th class="deploy-side-col">Type</th>
            <th>Path</th>
          </tr>
        </thead>
        <tbody>
          <For each={props.draft.excluded}>
            {(e, i) => (
              <tr classList={{ selected: row() === i() }} onFocusIn={() => setRow(i())}>
                <td class="deploy-side-col">{e.side === "local" ? "Local path" : "Deployment path"}</td>
                <td>
                  <input
                    class="deploy-cell-input mono"
                    value={e.path}
                    placeholder={e.side === "local" ? "storage" : "/public/uploads"}
                    spellcheck={false}
                    onInput={(ev) => props.update({ excluded: props.draft.excluded.map((x, j) => (j === i() ? { ...x, path: ev.currentTarget.value } : x)) })}
                  />
                </td>
              </tr>
            )}
          </For>
        </tbody>
      </table>
      <Show when={!props.draft.excluded.length}>
        <p class="ssh-hint">Nothing is excluded. Exclude items by name in Deployment | Options.</p>
      </Show>
    </div>
  );
}
