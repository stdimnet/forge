/**
 * Data Sources and Drivers dialog: list of project/global data sources on the left, the
 * selected one's General, SSH/SSL and Options tabs on the right, Test Connection (with the
 * SSH host key confirmation), OK/Cancel/Apply.
 */
import { createMemo, createResource, createSignal, For, Show } from "solid-js";
import { createStore, produce } from "solid-js/store";
import { CircleCheck, CircleX, Copy, Database, LoaderCircle, Minus, Plus, ShieldAlert } from "lucide-solid";
import {
  dbApi,
  defaultOptions,
  defaultSsh,
  DEFAULT_PORTS,
  DRIVER_LABELS,
  DRIVERS,
  DS_COLORS,
  type DataSource,
  type DataSourceInfo,
  type Driver,
  type SecretUpdate,
  type SshAuth,
  type SslMode,
  type TestResult,
} from "@/ipc/db";
import { remoteApi } from "@/ipc/remote";
import { isTauri } from "@/ipc";
import { workspace } from "@/state/workspace";
import { showBalloon } from "@/keymap";
import { ContextMenu, type MenuItem } from "./ContextMenu";
import { notifyDialogOpener } from "@/shell/window/dialogs";
import { closeDataSources, DATA_SOURCES_DIALOG, sources, type DataSourcesSaved, type DialogRequest } from "./store";

interface Draft {
  key: string;
  source: DataSource;
  /** `undefined` keeps the stored password. */
  password?: string;
  hadPassword: boolean;
  /** SSH password / key passphrase; `undefined` keeps the stored one. */
  sshSecret?: string;
  hadSshSecret: boolean;
  /** SSL client key passphrase; `undefined` keeps the stored one. */
  sslKey?: string;
  hadSslKey: boolean;
  dirty: boolean;
  isNew: boolean;
}

/** JDBC-style URL shown under the fields, like DataGrip. */
export function connectionUrl(ds: DataSource): string {
  if (ds.driver === "sqlite") return `jdbc:sqlite:${ds.file}`;
  const host = ds.host || "localhost";
  const port = ds.port ?? DEFAULT_PORTS[ds.driver];
  const hp = `${host}${port ? `:${port}` : ""}`;
  switch (ds.driver) {
    case "sqlserver":
      return `jdbc:sqlserver://${hp}${ds.database ? `;databaseName=${ds.database}` : ""}`;
    case "oracle":
      return `jdbc:oracle:thin:@//${hp}/${ds.database || "XEPDB1"}`;
    case "redis":
      return `redis://${hp}/${ds.database || "0"}`;
    case "mongodb":
      return host.startsWith("mongodb") ? host : `mongodb://${hp}${ds.database ? `/${ds.database}` : ""}`;
    default: {
      const scheme = ds.driver === "postgres" ? "postgresql" : ds.driver;
      return `jdbc:${scheme}://${hp}${ds.database ? `/${ds.database}` : ""}`;
    }
  }
}

/** Label of the database field per driver. */
const databaseLabel = (d: Driver) => (d === "oracle" ? "Service" : d === "redis" ? "Database index" : "Database");

const USERS: Partial<Record<Driver, string>> = { postgres: "postgres", sqlserver: "sa", clickhouse: "default", oracle: "system", mysql: "root", mariadb: "root" };

/** What the driver does with the SSL/TLS settings. */
const SSL_NOTES: Partial<Record<Driver, string>> = {
  sqlserver: "SQL Server: Prefer encrypts the login, Require encrypts everything without checking the certificate. The client key must not be encrypted.",
  clickhouse: "ClickHouse: TLS uses the HTTPS port (usually 8443). Verify CA also checks the host name.",
  redis: "Redis: TLS (rediss://) from Require on.",
  mongodb: "MongoDB: put the client certificate and its key in one PEM file and choose it as the client certificate.",
  oracle: "Oracle: SSL/TLS is configured in the Oracle client (sqlnet.ora / wallet).",
};

function draftName(ds: DataSource): string {
  if (ds.driver === "sqlite") return ds.file.split(/[\\/]/).pop() || "SQLite";
  const host = !ds.host || ds.host === "127.0.0.1" ? "localhost" : ds.host;
  return ds.database ? `${ds.database}@${host}` : `@${host}`;
}

function blank(driver: Driver): DataSource {
  return {
    id: "",
    name: "",
    driver,
    host: driver === "sqlite" ? "" : "localhost",
    port: DEFAULT_PORTS[driver] ?? null,
    database: driver === "redis" ? "0" : "",
    user: USERS[driver] ?? "",
    file: driver === "sqlite" ? "database/database.sqlite" : "",
    comment: "",
    scope: "project",
    options: defaultOptions(),
  };
}

const fromInfo = (i: DataSourceInfo): DataSource => {
  const { hasPassword: _h, hasSshSecret: _hs, hasSslPassphrase: _hp, summary: _s, via: _v, model: _m, transactions: _t, production: _p, ...ds } = i;
  const copy = structuredClone(ds);
  copy.options.ssh = { ...defaultSsh(), ...(copy.options.ssh ?? {}) };
  copy.options.ssl = { caFile: "", certFile: "", keyFile: "", ...(copy.options.ssl ?? {}) };
  return copy;
};

const draftOf = (s: DataSourceInfo): Draft => ({
  key: s.id,
  source: fromInfo(s),
  hadPassword: s.hasPassword,
  hadSshSecret: !!s.hasSshSecret,
  hadSslKey: !!s.hasSslPassphrase,
  dirty: false,
  isNew: false,
});

/** The secrets typed into the SSH/SSL tab (unchanged ones are left out). */
const secretsOf = (d: Draft): SecretUpdate => ({ ssh: d.sshSecret, sslKey: d.sslKey });

function Field(props: { label: string; children: unknown; wide?: boolean }) {
  return (
    <label class="dsd-field" classList={{ wide: props.wide }}>
      <span class="dsd-label">{props.label}:</span>
      {props.children as never}
    </label>
  );
}

/** Content of the Data Sources and Drivers dialog (the frame comes from `WindowFrame`). */
export function DataSourceDialog(p: { props: DialogRequest }) {
  const props = p.props ?? {};
  let seq = 0;
  const initial: Draft[] = sources().map(draftOf);
  const [drafts, setDrafts] = createStore<Draft[]>(initial);
  const [deleted, setDeleted] = createSignal<string[]>([]);
  const [selected, setSelected] = createSignal<string | null>(props.select ?? initial[0]?.key ?? null);
  const [tab, setTab] = createSignal<"general" | "ssh" | "options">(props.tab ?? "general");
  const [sshConfigs] = createResource(() => remoteApi.configs().catch(() => []));
  const [test, setTest] = createSignal<TestResult | "running" | null>(null);
  const [menu, setMenu] = createSignal<{ x: number; y: number; items: MenuItem[] } | null>(null);
  const [saving, setSaving] = createSignal(false);

  const addNew = (driver: Driver, base?: DataSource) => {
    const key = `new-${++seq}`;
    const source = base ? { ...structuredClone(base), id: "", name: `${base.name} (copy)` } : blank(driver);
    source.options.ssh = { ...defaultSsh(), ...(source.options.ssh ?? {}) };
    source.options.ssl = { caFile: "", certFile: "", keyFile: "", ...(source.options.ssl ?? {}) };
    setDrafts((d) => [...d, { key, source, hadPassword: false, hadSshSecret: false, hadSslKey: false, dirty: true, isNew: true, password: base ? undefined : "" }]);
    setSelected(key);
    setTab("general");
    setTest(null);
  };
  if (props.newDriver) addNew(props.newDriver);

  const index = createMemo(() => drafts.findIndex((d) => d.key === selected()));
  const cur = () => (index() >= 0 ? drafts[index()] : undefined);

  const update = (fn: (ds: DataSource) => void) => {
    const i = index();
    if (i < 0) return;
    setDrafts(
      i,
      produce((d) => {
        const autoName = !d.source.name || d.source.name === draftName(d.source);
        fn(d.source);
        if (autoName) d.source.name = draftName(d.source);
        d.dirty = true;
      }),
    );
    setTest(null);
  };

  const setPassword = (p: string) => {
    const i = index();
    if (i >= 0) setDrafts(i, { password: p, dirty: true });
    setTest(null);
  };

  const setSecret = (field: "sshSecret" | "sslKey", p: string) => {
    const i = index();
    if (i >= 0) setDrafts(i, { [field]: p, dirty: true });
    setTest(null);
  };

  const remove = () => {
    const d = cur();
    if (!d) return;
    if (!d.isNew) setDeleted((x) => [...x, d.source.id]);
    const i = index();
    setDrafts((list) => list.filter((x) => x.key !== d.key));
    setSelected(drafts[Math.min(i, drafts.length - 1)]?.key ?? null);
  };

  const runTest = async (acceptHostKey?: string) => {
    const d = cur();
    if (!d) return;
    setTest("running");
    try {
      setTest(await dbApi.test(d.source, d.password, secretsOf(d), acceptHostKey));
    } catch (e) {
      setTest({ ok: false, driver: DRIVER_LABELS[d.source.driver], latencyMs: 0, error: e instanceof Error ? e.message : String(e) });
    }
  };

  const apply = async (): Promise<boolean> => {
    setSaving(true);
    try {
      for (const id of deleted()) await dbApi.remove(id);
      setDeleted([]);
      const ids: string[] = [];
      for (let i = 0; i < drafts.length; i++) {
        const d = drafts[i];
        if (!d.dirty) continue;
        const saved = await dbApi.save(d.source, d.password, secretsOf(d));
        setDrafts(i, { ...draftOf(saved), password: undefined, sshSecret: undefined, sslKey: undefined });
        if (selected() === d.key) setSelected(saved.id);
        ids.push(saved.id);
      }
      // The opening window reloads the list and reconnects the saved sources it has open.
      notifyDialogOpener(DATA_SOURCES_DIALOG, { saved: ids } satisfies DataSourcesSaved);
      return true;
    } catch (e) {
      showBalloon("Cannot save data source", e instanceof Error ? e.message : String(e));
      return false;
    } finally {
      setSaving(false);
    }
  };

  const close = () => closeDataSources();
  const dirty = () => drafts.some((d) => d.dirty) || deleted().length > 0;

  const addMenu = (e: MouseEvent) => {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    setMenu({ x: r.left, y: r.bottom + 2, items: DRIVERS.map((d) => ({ label: DRIVER_LABELS[d], icon: <Database size={13} />, run: () => addNew(d) })) });
  };

  const browseFile = async () => {
    if (!isTauri()) return;
    const { open } = await import("@tauri-apps/plugin-dialog");
    const path = await open({ multiple: false, filters: [{ name: "SQLite", extensions: ["sqlite", "db", "sqlite3"] }] });
    if (typeof path === "string") update((s) => (s.file = path));
  };

  const group = (scope: "project" | "global") => drafts.filter((d) => d.source.scope === scope);

  const ListItem = (p: { d: Draft }) => (
    <button class="dsd-item" classList={{ selected: selected() === p.d.key }} onClick={() => (setSelected(p.d.key), setTest(null))}>
      <Database size={14} class={`dsd-driver ${p.d.source.driver}`} />
      <span class="dsd-item-name">{p.d.source.name || draftName(p.d.source)}</span>
      <Show when={p.d.dirty}>
        <span class="dsd-dirty" title="Modified">*</span>
      </Show>
    </button>
  );

  return (
    <div
      class="dsd-dialog"
      onKeyDown={(e) => {
        if (e.key === "Escape" && !menu()) {
          e.stopPropagation();
          close();
        }
      }}
    >
      <div class="dsd-body">
        <div class="dsd-sidebar">
          <div class="dsd-sidebar-tools">
            <button class="icon-btn" title="Add" onClick={addMenu}>
              <Plus size={15} />
            </button>
            <button class="icon-btn" title="Remove" disabled={!cur()} onClick={remove}>
              <Minus size={15} />
            </button>
            <button class="icon-btn" title="Duplicate" disabled={!cur()} onClick={() => cur() && addNew(cur()!.source.driver, cur()!.source)}>
              <Copy size={14} />
            </button>
          </div>
          <div class="dsd-list">
            <div class="dsd-group">Project Data Sources</div>
            <For each={group("project")} fallback={<div class="dsd-none muted">None</div>}>
              {(d) => <ListItem d={d} />}
            </For>
            <div class="dsd-group">Global Data Sources</div>
            <For each={group("global")} fallback={<div class="dsd-none muted">None</div>}>
              {(d) => <ListItem d={d} />}
            </For>
          </div>
        </div>
        <div class="dsd-main">
          <Show
            when={cur()}
            fallback={
              <div class="dsd-placeholder muted">
                <Database size={28} />
                <div>Select a data source or click + to add one</div>
              </div>
            }
          >
            {(d) => (
              <>
                <div class="dsd-head">
                  <Field label="Name" wide>
                    <input class="dsd-input" value={d().source.name} onInput={(e) => {
                      const i = index();
                      setDrafts(i, produce((x) => { x.source.name = e.currentTarget.value; x.dirty = true; }));
                    }} />
                  </Field>
                  <Field label="Comment" wide>
                    <input class="dsd-input" value={d().source.comment} onInput={(e) => update((s) => (s.comment = e.currentTarget.value))} />
                  </Field>
                </div>
                <div class="dsd-tabs" role="tablist">
                  <button class="dsd-tab" classList={{ active: tab() === "general" }} onClick={() => setTab("general")}>
                    General
                  </button>
                  <Show when={d().source.driver !== "sqlite"}>
                    <button class="dsd-tab" classList={{ active: tab() === "ssh" }} onClick={() => setTab("ssh")} data-testid="dsd-tab-ssh">
                      SSH/SSL
                    </button>
                  </Show>
                  <button class="dsd-tab" classList={{ active: tab() === "options" }} onClick={() => setTab("options")}>
                    Options
                  </button>
                </div>
                <div class="dsd-page">
                  <Show when={tab() === "general"}>
                    <Field label="Driver">
                      <select
                        class="dsd-input dsd-narrow"
                        value={d().source.driver}
                        onChange={(e) =>
                          update((s) => {
                            const driver = e.currentTarget.value as Driver;
                            if (s.port == null || s.port === DEFAULT_PORTS[s.driver]) s.port = DEFAULT_PORTS[driver] ?? null;
                            s.driver = driver;
                            if (driver === "sqlite" && !s.file) s.file = "database/database.sqlite";
                          })
                        }
                      >
                        <For each={DRIVERS}>{(dr) => <option value={dr}>{DRIVER_LABELS[dr]}</option>}</For>
                      </select>
                    </Field>
                    <Show
                      when={d().source.driver !== "sqlite"}
                      fallback={
                        <Field label="File">
                          <div class="dsd-row">
                            <input class="dsd-input" value={d().source.file} spellcheck={false} onInput={(e) => update((s) => (s.file = e.currentTarget.value))} />
                            <Show when={isTauri()}>
                              <button class="btn-secondary" onClick={() => void browseFile()}>
                                …
                              </button>
                            </Show>
                          </div>
                        </Field>
                      }
                    >
                      <div class="dsd-row-fields">
                        <Field label="Host">
                          <input class="dsd-input" value={d().source.host} spellcheck={false} onInput={(e) => update((s) => (s.host = e.currentTarget.value))} />
                        </Field>
                        <Field label="Port">
                          <input
                            class="dsd-input dsd-port"
                            value={d().source.port ?? ""}
                            inputMode="numeric"
                            onInput={(e) => update((s) => (s.port = e.currentTarget.value ? Number(e.currentTarget.value) || null : null))}
                          />
                        </Field>
                      </div>
                      <Show when={d().source.options.ssh?.enabled}>
                        <div class="dsd-hint muted" data-testid="dsd-ssh-note">
                          Connects through the SSH tunnel of the SSH/SSL tab: host and port as the SSH host sees them.
                        </div>
                      </Show>
                      <Show when={!d().source.options.ssh?.enabled && d().source.scope === "project" && workspace()?.remote}>
                        {(remote) => (
                          <div class="dsd-hint muted" data-testid="dsd-ssh-note">
                            Connects through the SSH connection to {remote().label}: host and port as the remote host sees them.
                          </div>
                        )}
                      </Show>
                      <Field label="Authentication">
                        <select class="dsd-input dsd-narrow" value="password" disabled>
                          <option value="password">User &amp; Password</option>
                        </select>
                      </Field>
                      <Field label="User">
                        <input class="dsd-input" value={d().source.user} spellcheck={false} onInput={(e) => update((s) => (s.user = e.currentTarget.value))} />
                      </Field>
                      <div class="dsd-row-fields">
                        <Field label="Password">
                          <input
                            class="dsd-input"
                            type="password"
                            placeholder={d().hadPassword && d().password === undefined ? "<hidden>" : ""}
                            value={d().password ?? ""}
                            onInput={(e) => setPassword(e.currentTarget.value)}
                          />
                        </Field>
                        <Field label="Save">
                          <select
                            class="dsd-input dsd-save"
                            value={d().source.options.savePassword ? "forever" : "never"}
                            onChange={(e) => update((s) => (s.options.savePassword = e.currentTarget.value === "forever"))}
                          >
                            <option value="forever">Forever</option>
                            <option value="never">Never</option>
                          </select>
                        </Field>
                      </div>
                      <Field label={databaseLabel(d().source.driver)}>
                        <input class="dsd-input" value={d().source.database} spellcheck={false} onInput={(e) => update((s) => (s.database = e.currentTarget.value))} />
                      </Field>
                      <Show when={d().source.driver === "oracle"}>
                        <div class="dsd-hint muted" data-testid="dsd-oracle-note">
                          Oracle runs through SQL*Plus (Oracle Instant Client) or SQLcl installed on this machine and found on PATH. Each statement runs on its own and is committed.
                        </div>
                      </Show>
                    </Show>
                    <Field label="URL">
                      <input class="dsd-input dsd-url" value={connectionUrl(d().source)} readOnly />
                    </Field>
                    <div class="dsd-hint muted">Passwords are stored in the system keychain, never in project files.</div>
                    <TestLine />
                  </Show>
                  <Show when={tab() === "ssh"}>
                    <SshSslPage d={d()} />
                    <TestLine />
                  </Show>
                  <Show when={tab() === "options"}>
                    <OptionsPage d={d()} />
                  </Show>
                </div>
              </>
            )}
          </Show>
        </div>
      </div>
      <div class="dsd-footer">
        <span class="dsd-spacer" />
        <button class="btn-primary" disabled={saving()} onClick={() => void apply().then((ok) => ok && close())}>
          OK
        </button>
        <button class="btn-secondary" onClick={close}>
          Cancel
        </button>
        <button class="btn-secondary" disabled={!dirty() || saving()} onClick={() => void apply()}>
          Apply
        </button>
      </div>
      <Show when={menu()}>{(m) => <ContextMenu {...m()} onClose={() => setMenu(null)} />}</Show>
    </div>
  );

  function TestLine() {
    return (
                    <div class="dsd-test">
                      <button class="dsd-test-link" disabled={test() === "running"} onClick={() => void runTest()}>
                        Test Connection
                      </button>
                      <Show when={test()}>
                        {(t) => (
                          <Show
                            when={t() !== "running"}
                            fallback={
                              <span class="dsd-test-status muted">
                                <LoaderCircle size={14} class="db-spin" /> Connecting…
                              </span>
                            }
                          >
                            {(() => {
                              const r = t() as TestResult;
                              return r.ok ? (
                                <span class="dsd-test-status ok">
                                  <CircleCheck size={15} />
                                  <span>
                                    <b>Succeeded</b>
                                    <span class="muted">
                                      {" "}
                                      · {r.driver} {r.serverVersion} · {r.latencyMs} ms
                                    </span>
                                  </span>
                                </span>
                              ) : (
                                <span class="dsd-test-status error">
                                  {r.hostKey ? <ShieldAlert size={15} /> : <CircleX size={15} />}
                                  <span>
                                    <b>Failed</b> <span class="dsd-test-error">{r.error}</span>
                                    <Show when={r.hostKey}>
                                      {(k) => (
                                        <button class="dsd-test-link dsd-trust" data-testid="dsd-trust-host" onClick={() => void runTest(k().fingerprint)}>
                                          Trust {k().keyType} key {k().fingerprint} and test again
                                        </button>
                                      )}
                                    </Show>
                                  </span>
                                </span>
                              );
                            })()}
                          </Show>
                        )}
                      </Show>
                    </div>
    );
  }

  function SshSslPage(p: { d: Draft }) {
    const ssh = () => p.d.source.options.ssh ?? defaultSsh();
    const ssl = () => p.d.source.options.ssl ?? {};
    const adhoc = () => !ssh().configurationId;
    const setSsh = (fn: (t: NonNullable<DataSource["options"]["ssh"]>) => void) =>
      update((s) => {
        s.options.ssh = { ...defaultSsh(), ...(s.options.ssh ?? {}) };
        fn(s.options.ssh);
      });
    const setSsl = (fn: (f: NonNullable<DataSource["options"]["ssl"]>) => void) =>
      update((s) => {
        s.options.ssl = { ...(s.options.ssl ?? {}) };
        fn(s.options.ssl);
      });
    const secretLabel = () => (ssh().auth === "keyPair" ? "Passphrase" : "Password");
    const browse = async (apply: (path: string) => void) => {
      if (!isTauri()) return;
      const { open } = await import("@tauri-apps/plugin-dialog");
      const path = await open({ multiple: false });
      if (typeof path === "string") apply(path);
    };
    const FileField = (f: { label: string; value: string; testid: string; set: (v: string) => void }) => (
      <Field label={f.label}>
        <div class="dsd-row">
          <input class="dsd-input" value={f.value} spellcheck={false} data-testid={f.testid} placeholder="PEM file" onInput={(e) => f.set(e.currentTarget.value)} />
          <Show when={isTauri()}>
            <button class="btn-secondary" onClick={() => void browse(f.set)}>
              …
            </button>
          </Show>
        </div>
      </Field>
    );
    return (
      <div class="dsd-ssh" data-testid="dsd-ssh-page">
        <div class="dsd-section">SSH</div>
        <label class="dsd-check">
          <input type="checkbox" checked={ssh().enabled} data-testid="dsd-use-ssh" onChange={(e) => setSsh((t) => (t.enabled = e.currentTarget.checked))} />
          Use SSH tunnel
        </label>
        <div class="dsd-indent" classList={{ disabled: !ssh().enabled }}>
          <Field label="SSH configuration">
            <select
              class="dsd-input"
              disabled={!ssh().enabled}
              value={ssh().configurationId ?? ""}
              onChange={(e) => setSsh((t) => (t.configurationId = e.currentTarget.value || null))}
            >
              <option value="">Ad-hoc (settings below)</option>
              <For each={sshConfigs() ?? []}>{(c) => <option value={c.id}>{c.name || c.label}</option>}</For>
            </select>
          </Field>
          <Show when={adhoc()}>
            <div class="dsd-row-fields">
              <Field label="Host">
                <input class="dsd-input" disabled={!ssh().enabled} value={ssh().host} spellcheck={false} placeholder="bastion.example.com or ~/.ssh/config alias" onInput={(e) => setSsh((t) => (t.host = e.currentTarget.value))} />
              </Field>
              <Field label="Port">
                <input class="dsd-input dsd-port" disabled={!ssh().enabled} value={ssh().port} inputMode="numeric" onInput={(e) => setSsh((t) => (t.port = Number(e.currentTarget.value) || 22))} />
              </Field>
            </div>
            <Field label="User name">
              <input class="dsd-input" disabled={!ssh().enabled} value={ssh().user} spellcheck={false} onInput={(e) => setSsh((t) => (t.user = e.currentTarget.value))} />
            </Field>
            <Field label="Authentication type">
              <select class="dsd-input" disabled={!ssh().enabled} value={ssh().auth} onChange={(e) => setSsh((t) => (t.auth = e.currentTarget.value as SshAuth))}>
                <option value="password">Password</option>
                <option value="keyPair">Key pair</option>
                <option value="agent">OpenSSH config and authentication agent</option>
              </select>
            </Field>
            <Show when={ssh().auth === "keyPair"}>
              <FileField label="Private key file" value={ssh().keyFile ?? ""} testid="dsd-ssh-key" set={(v) => setSsh((t) => (t.keyFile = v || null))} />
            </Show>
            <Show when={ssh().auth !== "agent"}>
              <Field label={secretLabel()}>
                <input
                  class="dsd-input"
                  type="password"
                  disabled={!ssh().enabled}
                  placeholder={p.d.hadSshSecret && p.d.sshSecret === undefined ? "<hidden>" : ""}
                  value={p.d.sshSecret ?? ""}
                  onInput={(e) => setSecret("sshSecret", e.currentTarget.value)}
                />
              </Field>
            </Show>
          </Show>
          <Show when={!adhoc()}>
            <div class="dsd-hint muted">Host, user and authentication come from the SSH configuration (Settings | Tools | SSH Configurations).</div>
          </Show>
        </div>
        <div class="dsd-section">SSL</div>
        <Field label="Mode">
          <select class="dsd-input dsd-narrow" value={p.d.source.options.sslMode} data-testid="dsd-ssl-mode" onChange={(e) => update((s) => (s.options.sslMode = e.currentTarget.value as SslMode))}>
            <option value="disable">Disable</option>
            <option value="prefer">Prefer</option>
            <option value="require">Require</option>
            <option value="verify-ca">Verify CA</option>
            <option value="verify-full">Verify Full</option>
          </select>
        </Field>
        <div class="dsd-indent" classList={{ disabled: p.d.source.options.sslMode === "disable" }}>
          <FileField label="CA file" value={ssl().caFile ?? ""} testid="dsd-ssl-ca" set={(v) => setSsl((f) => (f.caFile = v))} />
          <FileField label="Client certificate file" value={ssl().certFile ?? ""} testid="dsd-ssl-cert" set={(v) => setSsl((f) => (f.certFile = v))} />
          <FileField label="Client key file" value={ssl().keyFile ?? ""} testid="dsd-ssl-key" set={(v) => setSsl((f) => (f.keyFile = v))} />
          <Field label="Client key password">
            <input
              class="dsd-input"
              type="password"
              placeholder={p.d.hadSslKey && p.d.sslKey === undefined ? "<hidden>" : ""}
              value={p.d.sslKey ?? ""}
              onInput={(e) => setSecret("sslKey", e.currentTarget.value)}
            />
          </Field>
        </div>
        <Show when={SSL_NOTES[p.d.source.driver]}>{(n) => <div class="dsd-hint muted">{n()}</div>}</Show>
        <div class="dsd-hint muted">Relative certificate paths are resolved against the project. Secrets are stored in the system keychain.</div>
      </div>
    );
  }

  function OptionsPage(p: { d: Draft }) {
    const d = () => p.d;
    const transactions = () => ["mysql", "mariadb", "postgres", "sqlite", "sqlserver"].includes(d().source.driver);
    return (
      <>
        <div class="dsd-section">Connection</div>
        <label class="dsd-check">
          <input type="checkbox" checked={d().source.options.readOnly} data-testid="dsd-read-only" onChange={(e) => update((s) => (s.options.readOnly = e.currentTarget.checked))} />
          Read-only
        </label>
        <Field label="Color">
          <div class="dsd-row">
            <span class="dsd-swatch" style={{ background: DS_COLORS[d().source.options.color ?? ""]?.accent ?? "transparent" }} />
            <select
              class="dsd-input dsd-narrow"
              value={d().source.options.color ?? ""}
              data-testid="dsd-color"
              onChange={(e) => update((s) => (s.options.color = e.currentTarget.value || null))}
            >
              <option value="">None</option>
              <For each={Object.entries(DS_COLORS)}>{([id, c]) => <option value={id}>{c.label}</option>}</For>
            </select>
          </div>
        </Field>
        <Show when={d().source.options.color === "red"}>
          <div class="dsd-hint muted">Production: consoles ask before UPDATE or DELETE without WHERE (and TRUNCATE, FLUSHDB, deleteMany({})).</div>
        </Show>
        <Show when={transactions()}>
          <Field label="Transaction control">
            <select
              class="dsd-input dsd-narrow"
              value={d().source.options.autoCommit ? "auto" : "manual"}
              onChange={(e) => update((s) => (s.options.autoCommit = e.currentTarget.value === "auto"))}
            >
              <option value="auto">Auto</option>
              <option value="manual">Manual</option>
            </select>
          </Field>
        </Show>
        <label class="dsd-check">
          <input type="checkbox" checked={!!d().source.options.singleSession} onChange={(e) => update((s) => (s.options.singleSession = e.currentTarget.checked))} />
          Single session mode (all consoles share one connection)
        </label>
        <Field label="Connect timeout (s)">
          <input
            class="dsd-input dsd-port"
            value={d().source.options.connectTimeoutSecs}
            onInput={(e) => update((s) => (s.options.connectTimeoutSecs = Math.max(1, Number(e.currentTarget.value) || 10)))}
          />
        </Field>
        <Show when={d().source.driver !== "redis" && d().source.driver !== "mongodb"}>
          <Field label="Startup script">
            <textarea
              class="dsd-input dsd-textarea"
              spellcheck={false}
              placeholder="SET time_zone = '+00:00';"
              value={d().source.options.initSql}
              onInput={(e) => update((s) => (s.options.initSql = e.currentTarget.value))}
            />
          </Field>
        </Show>
        <div class="dsd-section">Data Views</div>
        <Field label="Page size">
          <input
            class="dsd-input dsd-port"
            placeholder="500"
            value={d().source.options.pageSize ?? ""}
            onInput={(e) => update((s) => (s.options.pageSize = Number(e.currentTarget.value) || null))}
          />
        </Field>
        <Field label={d().source.driver === "mongodb" || d().source.driver === "redis" ? "Databases" : "Schemas"}>
          <input
            class="dsd-input"
            placeholder="current schema; * for all"
            value={d().source.options.schemas.join(", ")}
            onInput={(e) => update((s) => (s.options.schemas = e.currentTarget.value.split(",").map((x) => x.trim()).filter(Boolean)))}
          />
        </Field>
        <div class="dsd-section">Scope</div>
        <label class="dsd-check">
          <input type="checkbox" checked={d().source.scope === "global"} onChange={(e) => update((s) => (s.scope = e.currentTarget.checked ? "global" : "project"))} />
          Global (available in all projects)
        </label>
      </>
    );
  }
}
