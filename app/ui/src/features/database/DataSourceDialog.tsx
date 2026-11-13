/**
 * Data Sources and Drivers dialog: list of project/global data sources on the left, the
 * selected one's General, SSH/SSL, Schemas and Options tabs on the right, Test Connection
 * (with the SSH host key confirmation), OK/Cancel/Apply. DDL data sources list SQL files of
 * the project instead of a connection.
 */
import { createEffect, createMemo, createResource, createSignal, For, on, Show } from "solid-js";
import { createStore, produce } from "solid-js/store";
import { CircleCheck, CircleX, Copy, Database, FileCode, LoaderCircle, Minus, Plus, ShieldAlert } from "lucide-solid";
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
  type Dialect,
  type Driver,
  type SchemaName,
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
import { t as tr } from "@/i18n";

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
  if (ds.driver === "ddl") return `ddl:${ds.options.ddlDialect ?? "postgres"}:${(ds.options.ddlFiles ?? []).join(",")}`;
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
const databaseLabel = (d: Driver) => (d === "oracle" ? tr("database.dataSourceDialog.service") : d === "redis" ? tr("database.dataSourceDialog.databaseIndex") : tr("database.dataSourceDialog.database"));

const USERS: Partial<Record<Driver, string>> = { postgres: "postgres", sqlserver: "sa", clickhouse: "default", oracle: "system", mysql: "root", mariadb: "root" };

/** What the driver does with the SSL/TLS settings. */
const SSL_NOTES: Partial<Record<Driver, string>> = {
  sqlserver: "SQL Server: Prefer encrypts the login only, Require skips the certificate check; the client key must be unencrypted.",
  clickhouse: "ClickHouse: TLS uses the HTTPS port (usually 8443). Verify CA also checks the host name.",
  redis: "Redis: TLS (rediss://) from Require on.",
  mongodb: "MongoDB: put the client certificate and its key in one PEM file and choose it as the client certificate.",
  oracle: "Oracle: SSL/TLS is configured in the Oracle client (sqlnet.ora / wallet).",
};

function draftName(ds: DataSource): string {
  if (ds.driver === "sqlite") return ds.file.split(/[\\/]/).pop() || "SQLite";
  if (ds.driver === "ddl") return (ds.options.ddlFiles?.[0] ?? "").split(/[\\/]/).filter(Boolean).pop() || "DDL";
  const host = !ds.host || ds.host === "127.0.0.1" ? "localhost" : ds.host;
  return ds.database ? `${ds.database}@${host}` : `@${host}`;
}

function blank(driver: Driver): DataSource {
  return {
    id: "",
    name: "",
    driver,
    host: driver === "sqlite" || driver === "ddl" ? "" : "localhost",
    port: DEFAULT_PORTS[driver] ?? null,
    database: driver === "redis" ? "0" : "",
    user: USERS[driver] ?? "",
    file: driver === "sqlite" ? "database/database.sqlite" : "",
    comment: "",
    scope: "project",
    options: driver === "ddl" ? { ...defaultOptions(), ddlFiles: ["database/schema.sql"], ddlDialect: "postgres" } : defaultOptions(),
  };
}

/** Dialects DDL files can be written in. */
const DDL_DIALECTS: { id: Dialect; label: string }[] = [
  { id: "postgres", label: tr("tools.database.postgresql") },
  { id: "mysql", label: tr("tools.database.mysqlMariadb") },
  { id: "sqlite", label: tr("tools.database.sqlite") },
  { id: "sqlserver", label: tr("tools.database.sqlServer") },
  { id: "oracle", label: tr("tools.database.oracle") },
  { id: "clickhouse", label: tr("tools.database.clickhouse") },
];

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
  const [tab, setTab] = createSignal<"general" | "ssh" | "schemas" | "options">(props.tab ?? "general");
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
      showBalloon(tr("database.dataSourceDialog.cannotSaveDataSource"), e instanceof Error ? e.message : String(e));
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
        <span class="dsd-dirty" title={tr("database.dataSourceDialog.modified")}>*</span>
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
            <button class="icon-btn" title={tr("common.add")} onClick={addMenu}>
              <Plus size={15} />
            </button>
            <button class="icon-btn" title={tr("common.remove")} disabled={!cur()} onClick={remove}>
              <Minus size={15} />
            </button>
            <button class="icon-btn" title={tr("database.dataSourceDialog.duplicate")} disabled={!cur()} onClick={() => cur() && addNew(cur()!.source.driver, cur()!.source)}>
              <Copy size={14} />
            </button>
          </div>
          <div class="dsd-list">
            <div class="dsd-group">{tr("database.dataSourceDialog.projectDataSources")}</div>
            <For each={group("project")} fallback={<div class="dsd-none muted">{tr("database.dataSourceDialog.none")}</div>}>
              {(d) => <ListItem d={d} />}
            </For>
            <div class="dsd-group">{tr("database.dataSourceDialog.globalDataSources")}</div>
            <For each={group("global")} fallback={<div class="dsd-none muted">{tr("database.dataSourceDialog.none")}</div>}>
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
                <div>{tr("database.dataSourceDialog.selectADataSourceOr")}</div>
              </div>
            }
          >
            {(d) => (
              <>
                <div class="dsd-head">
                  <Field label={tr("database.dataSourceDialog.name")} wide>
                    <input class="dsd-input" value={d().source.name} onInput={(e) => {
                      const i = index();
                      setDrafts(i, produce((x) => { x.source.name = e.currentTarget.value; x.dirty = true; }));
                    }} />
                  </Field>
                  <Field label={tr("database.dataSourceDialog.comment")} wide>
                    <input class="dsd-input" value={d().source.comment} onInput={(e) => update((s) => (s.comment = e.currentTarget.value))} />
                  </Field>
                </div>
                <div class="dsd-tabs" role="tablist">
                  <button class="dsd-tab" classList={{ active: tab() === "general" }} onClick={() => setTab("general")}>
                    {tr("database.dataSourceDialog.general")}
                  </button>
                  <Show when={d().source.driver !== "sqlite" && d().source.driver !== "ddl"}>
                    <button class="dsd-tab" classList={{ active: tab() === "ssh" }} onClick={() => setTab("ssh")} data-testid="dsd-tab-ssh">
                      {tr("tools.database.sshSsl")}
                    </button>
                  </Show>
                  <button class="dsd-tab" classList={{ active: tab() === "schemas" }} onClick={() => setTab("schemas")} data-testid="dsd-tab-schemas">
                    {d().source.driver === "mongodb" || d().source.driver === "redis" ? tr("database.dataSourceDialog.databases") : tr("database.dataSourceDialog.schemas")}
                  </button>
                  <button class="dsd-tab" classList={{ active: tab() === "options" }} onClick={() => setTab("options")}>
                    {tr("database.dataSourceDialog.options")}
                  </button>
                </div>
                <div class="dsd-page">
                  <Show when={tab() === "general"}>
                    <Field label={tr("database.dataSourceDialog.driver")}>
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
                    <Show when={d().source.driver === "ddl"}>
                      <DdlPage d={d()} />
                    </Show>
                    <Show
                      when={d().source.driver !== "sqlite" && d().source.driver !== "ddl"}
                      fallback={
                        <Show when={d().source.driver === "sqlite"}>
                        <Field label={tr("database.dataSourceDialog.file")}>
                          <div class="dsd-row">
                            <input class="dsd-input" value={d().source.file} spellcheck={false} onInput={(e) => update((s) => (s.file = e.currentTarget.value))} />
                            <Show when={isTauri()}>
                              <button class="btn-secondary" onClick={() => void browseFile()}>
                                …
                              </button>
                            </Show>
                          </div>
                        </Field>
                        </Show>
                      }
                    >
                      <div class="dsd-row-fields">
                        <Field label={tr("database.dataSourceDialog.host")}>
                          <input class="dsd-input" value={d().source.host} spellcheck={false} onInput={(e) => update((s) => (s.host = e.currentTarget.value))} />
                        </Field>
                        <Field label={tr("database.dataSourceDialog.port")}>
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
                          {tr("database.dataSourceDialog.connectsThroughTheSshTunnel")}
                        </div>
                      </Show>
                      <Show when={!d().source.options.ssh?.enabled && d().source.scope === "project" && workspace()?.remote}>
                        {(remote) => (
                          <div class="dsd-hint muted" data-testid="dsd-ssh-note">
                            {tr("tools.database.connectsThroughTheSshConnection", { label: remote().label })}
                          </div>
                        )}
                      </Show>
                      <Field label={tr("database.dataSourceDialog.authentication")}>
                        <select class="dsd-input dsd-narrow" value="password" disabled>
                          <option value="password">{tr("tools.database.userAmpPassword")}</option>
                        </select>
                      </Field>
                      <Field label={tr("database.dataSourceDialog.user")}>
                        <input class="dsd-input" value={d().source.user} spellcheck={false} onInput={(e) => update((s) => (s.user = e.currentTarget.value))} />
                      </Field>
                      <div class="dsd-row-fields">
                        <Field label={tr("database.dataSourceDialog.password")}>
                          <input
                            class="dsd-input"
                            type="password"
                            placeholder={d().hadPassword && d().password === undefined ? tr("tools.database.hidden") : ""}
                            value={d().password ?? ""}
                            onInput={(e) => setPassword(e.currentTarget.value)}
                          />
                        </Field>
                        <Field label={tr("database.dataSourceDialog.save")}>
                          <select
                            class="dsd-input dsd-save"
                            value={d().source.options.savePassword ? "forever" : "never"}
                            onChange={(e) => update((s) => (s.options.savePassword = e.currentTarget.value === "forever"))}
                          >
                            <option value="forever">{tr("database.dataSourceDialog.forever")}</option>
                            <option value="never">{tr("database.dataSourceDialog.never")}</option>
                          </select>
                        </Field>
                      </div>
                      <Field label={databaseLabel(d().source.driver)}>
                        <input class="dsd-input" value={d().source.database} spellcheck={false} onInput={(e) => update((s) => (s.database = e.currentTarget.value))} />
                      </Field>
                      <Show when={d().source.driver === "oracle"}>
                        <div class="dsd-hint muted" data-testid="dsd-oracle-note">
                          {tr("tools.database.oracleRunsThroughSqlPlus")}
                        </div>
                      </Show>
                    </Show>
                    <Field label="URL">
                      <input class="dsd-input dsd-url" value={connectionUrl(d().source)} readOnly />
                    </Field>
                    <Show when={d().source.driver !== "ddl"}>
                      <div class="dsd-hint muted">{tr("database.dataSourceDialog.passwordsAreStoredInThe")}</div>
                      <TestLine />
                    </Show>
                  </Show>
                  <Show when={tab() === "ssh"}>
                    <SshSslPage d={d()} />
                    <TestLine />
                  </Show>
                  <Show when={tab() === "schemas"}>
                    <SchemasPage d={d()} />
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
          {tr("common.cancel")}
        </button>
        <button class="btn-secondary" disabled={!dirty() || saving()} onClick={() => void apply()}>
          {tr("common.apply")}
        </button>
      </div>
      <Show when={menu()}>{(m) => <ContextMenu {...m()} onClose={() => setMenu(null)} />}</Show>
    </div>
  );

  /** DDL data source: SQL files and folders of the project, their dialect and schema. */
  function DdlPage(p: { d: Draft }) {
    const files = () => p.d.source.options.ddlFiles ?? [];
    const browse = async () => {
      if (!isTauri()) return;
      const { open } = await import("@tauri-apps/plugin-dialog");
      const picked = await open({ multiple: true, filters: [{ name: "SQL", extensions: ["sql", "ddl"] }] });
      const list = Array.isArray(picked) ? picked : typeof picked === "string" ? [picked] : [];
      const root = workspace()?.root ?? "";
      const rel = list.map((x) => (root && x.startsWith(`${root}/`) ? x.slice(root.length + 1) : x));
      if (rel.length) update((s) => (s.options.ddlFiles = [...files(), ...rel]));
    };
    return (
      <>
        <Field label={tr("database.dataSourceDialog.files")}>
          <div class="dsd-ddl-files">
            <textarea
              class="dsd-input dsd-textarea"
              spellcheck={false}
              data-testid="dsd-ddl-files"
              placeholder={tr("tools.database.databaseSchemaSqlDbMigrations")}
              value={files().join("\n")}
              onInput={(e) => update((s) => (s.options.ddlFiles = e.currentTarget.value.split("\n").map((x) => x.trim()).filter(Boolean)))}
            />
            <Show when={isTauri()}>
              <button class="btn-secondary" onClick={() => void browse()}>
                <FileCode size={13} /> {tr("database.dataSourceDialog.addFiles")}
              </button>
            </Show>
          </div>
        </Field>
        <Field label={tr("database.dataSourceDialog.dialect")}>
          <select class="dsd-input dsd-narrow" value={p.d.source.options.ddlDialect ?? "postgres"} onChange={(e) => update((s) => (s.options.ddlDialect = e.currentTarget.value as Dialect))}>
            <For each={DDL_DIALECTS}>{(x) => <option value={x.id}>{x.label}</option>}</For>
          </select>
        </Field>
        <Field label={tr("database.dataSourceDialog.defaultSchema")}>
          <input class="dsd-input dsd-narrow" placeholder="public" value={p.d.source.database} spellcheck={false} onInput={(e) => update((s) => (s.database = e.currentTarget.value))} />
        </Field>
        <div class="dsd-hint muted" data-testid="dsd-ddl-note">
          {tr("tools.database.oneFileOrFolderPer", { value: tr("database.dataSourceDialog.forgeParsesTheFilesAgain") })}
        </div>
      </>
    );
  }

  /** The schema filter (DataGrip's Schemas tab): patterns and the schemas they select. */
  function SchemasPage(p: { d: Draft }) {
    const patterns = () => p.d.source.options.schemas;
    const setPatterns = (list: string[]) => update((s) => (s.options.schemas = list));
    const [known, setKnown] = createSignal<SchemaName[] | "loading" | string>([]);
    const [preview, setPreview] = createSignal<SchemaName[]>([]);
    const [error, setError] = createSignal<string>("");
    createEffect(
      on(
        () => (p.d.isNew ? "" : p.d.source.id),
        (id) => {
          setKnown(id ? "loading" : []);
          if (id)
            void dbApi
              .schemas(id)
              .then((l) => setKnown(l))
              .catch((e) => setKnown(e instanceof Error ? e.message : String(e)));
        },
      ),
    );
    createEffect(() => {
      const list = known();
      const pats = [...patterns()];
      if (!Array.isArray(list)) return setPreview([]);
      dbApi
        .matchSchemas(pats, list)
        .then((r) => {
          setPreview(r);
          setError("");
        })
        .catch((e) => setError(e instanceof Error ? e.message : String(e)));
    });
    const toggle = (s: SchemaName, on: boolean) => {
      let list = patterns().filter((x) => x !== s.name && x !== `!${s.name}`);
      if (on && !preview().find((x) => x.name === s.name)?.visible) list = [...list, s.name];
      if (!on) list = [...list, `!${s.name}`];
      // A lone exclusion would show every other schema: keep the current one as the base.
      if (list.length && list.every((x) => x.startsWith("!"))) list = [":current", ...list];
      setPatterns(list);
    };
    const flag = (pattern: string) => patterns().includes(pattern);
    const setFlag = (pattern: string, on: boolean) => setPatterns(on ? [...patterns().filter((x) => x !== pattern), pattern] : patterns().filter((x) => x !== pattern));
    return (
      <div class="dsd-schemas" data-testid="dsd-schemas">
        <div class="dsd-row-fields">
          <label class="dsd-check">
            <input type="checkbox" checked={flag("*")} onChange={(e) => setFlag("*", e.currentTarget.checked)} />
            {tr("database.dataSourceDialog.allSchemas")}
          </label>
          <label class="dsd-check">
            <input type="checkbox" checked={flag(":current") || !patterns().length} onChange={(e) => setFlag(":current", e.currentTarget.checked)} />
            {tr("database.dataSourceDialog.currentSchema")}
          </label>
        </div>
        <Field label={tr("database.dataSourceDialog.patterns")}>
          <textarea
            class="dsd-input dsd-textarea dsd-patterns"
            spellcheck={false}
            data-testid="dsd-schema-patterns"
            placeholder=":current"
            value={patterns().join("\n")}
            onInput={(e) => setPatterns(e.currentTarget.value.split(/[\n,]/).map((x) => x.trim()).filter(Boolean))}
          />
        </Field>
        <div class="dsd-hint muted">
          {tr("tools.database.onePatternPerLineA")} <code>*</code> and <code>?</code> {tr("tools.database.wildcards")}<code>{"tenant_*"}</code>), <code>:current</code>, <code>/regex/</code> (<code>/regex/i</code> {tr("tools.database.ignoresCaseAnd")}
          <code>{"!pattern"}</code> {tr("tools.database.toExcludeNoPatternMeans")}
        </div>
        <Show when={error()}>
          <div class="dsd-hint dsd-error">{error()}</div>
        </Show>
        <div class="dsd-schema-list">
          <Show
            when={Array.isArray(known())}
            fallback={
              <div class="muted dsd-schema-status">
                {known() === "loading" ? (
                  <>
                    <LoaderCircle size={13} class="db-spin" /> {tr("database.dataSourceDialog.loadingSchemas")}
                  </>
                ) : (
                  tr("database.dataSourceDialog.cannotListSchemasKnownasstring", { knownasstring: known() as string })
                )}
              </div>
            }
          >
            <Show when={preview().length} fallback={<div class="muted dsd-schema-status">{p.d.isNew ? tr("database.dataSourceDialog.applyTheDataSourceTo") : tr("database.dataSourceDialog.noSchemas")}</div>}>
              <For each={preview()}>
                {(s) => (
                  <label class="dsd-schema-item" classList={{ hidden: !s.visible }}>
                    <input type="checkbox" checked={!!s.visible} onChange={(e) => toggle(s, e.currentTarget.checked)} />
                    <span classList={{ strong: s.current }}>{s.name}</span>
                    <Show when={s.current}>
                      <span class="muted">current</span>
                    </Show>
                  </label>
                )}
              </For>
            </Show>
          </Show>
        </div>
        <div class="dsd-hint muted">
          {Array.isArray(known()) ? `${preview().filter((s) => s.visible).length} of ${preview().length} shown` : ""}
        </div>
      </div>
    );
  }

  function TestLine() {
    return (
                    <div class="dsd-test">
                      <button class="dsd-test-link" disabled={test() === "running"} onClick={() => void runTest()}>
                        {tr("database.dataSourceDialog.testConnection")}
                      </button>
                      <Show when={test()}>
                        {(t) => (
                          <Show
                            when={t() !== "running"}
                            fallback={
                              <span class="dsd-test-status muted">
                                <LoaderCircle size={14} class="db-spin" /> {tr("database.dataSourceDialog.connecting")}
                              </span>
                            }
                          >
                            {(() => {
                              const r = t() as TestResult;
                              return r.ok ? (
                                <span class="dsd-test-status ok">
                                  <CircleCheck size={15} />
                                  <span>
                                    <b>{tr("database.dataSourceDialog.succeeded")}</b>
                                    <span class="muted">
                                      {tr("tools.database.ms2", { driver: r.driver, serverVersion: r.serverVersion, latencyMs: r.latencyMs })}
                                    </span>
                                  </span>
                                </span>
                              ) : (
                                <span class="dsd-test-status error">
                                  {r.hostKey ? <ShieldAlert size={15} /> : <CircleX size={15} />}
                                  <Show
                                    when={r.hostKey}
                                    fallback={
                                      <span>
                                        <b>{tr("database.dataSourceDialog.failed")}</b> <span class="dsd-test-error">{r.error}</span>
                                      </span>
                                    }
                                  >
                                    {(k) => (
                                      <span>
                                        <b>{tr("database.dataSourceDialog.unknownSshHostKey")}</b>{" "}
                                        <span class="dsd-test-error">
                                          {tr("tools.database.presented", { host: k().host, port: k().port, keyType: k().keyType, fingerprint: k().fingerprint })}
                                        </span>
                                        <button class="dsd-test-link dsd-trust" data-testid="dsd-trust-host" onClick={() => void runTest(k().fingerprint)}>
                                          {tr("database.dataSourceDialog.trustHostKeyAndTest")}
                                        </button>
                                      </span>
                                    )}
                                  </Show>
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
    const secretLabel = () => (ssh().auth === "keyPair" ? tr("database.dataSourceDialog.passphrase") : tr("database.dataSourceDialog.password"));
    const browse = async (apply: (path: string) => void) => {
      if (!isTauri()) return;
      const { open } = await import("@tauri-apps/plugin-dialog");
      const path = await open({ multiple: false });
      if (typeof path === "string") apply(path);
    };
    const FileField = (f: { label: string; value: string; testid: string; set: (v: string) => void }) => (
      <Field label={f.label}>
        <div class="dsd-row">
          <input class="dsd-input" value={f.value} spellcheck={false} data-testid={f.testid} placeholder={tr("database.dataSourceDialog.pemFile")} onInput={(e) => f.set(e.currentTarget.value)} />
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
          {tr("database.dataSourceDialog.useSshTunnel")}
        </label>
        <div class="dsd-indent" classList={{ disabled: !ssh().enabled }}>
          <Field label={tr("database.dataSourceDialog.sshConfiguration")}>
            <select
              class="dsd-input"
              disabled={!ssh().enabled}
              value={ssh().configurationId ?? ""}
              onChange={(e) => setSsh((t) => (t.configurationId = e.currentTarget.value || null))}
            >
              <option value="">{tr("tools.database.adHocSettingsBelow")}</option>
              <For each={sshConfigs() ?? []}>{(c) => <option value={c.id}>{c.name || c.label}</option>}</For>
            </select>
          </Field>
          <Show when={adhoc()}>
            <div class="dsd-row-fields">
              <Field label={tr("database.dataSourceDialog.host")}>
                <input class="dsd-input" disabled={!ssh().enabled} value={ssh().host} spellcheck={false} placeholder={tr("database.dataSourceDialog.bastionExampleComOrSsh")} onInput={(e) => setSsh((t) => (t.host = e.currentTarget.value))} />
              </Field>
              <Field label={tr("database.dataSourceDialog.port")}>
                <input class="dsd-input dsd-port" disabled={!ssh().enabled} value={ssh().port} inputMode="numeric" onInput={(e) => setSsh((t) => (t.port = Number(e.currentTarget.value) || 22))} />
              </Field>
            </div>
            <Field label={tr("database.dataSourceDialog.userName")}>
              <input class="dsd-input" disabled={!ssh().enabled} value={ssh().user} spellcheck={false} onInput={(e) => setSsh((t) => (t.user = e.currentTarget.value))} />
            </Field>
            <Field label={tr("database.dataSourceDialog.authenticationType")}>
              <select class="dsd-input" disabled={!ssh().enabled} value={ssh().auth} onChange={(e) => setSsh((t) => (t.auth = e.currentTarget.value as SshAuth))}>
                <option value="password">{tr("database.dataSourceDialog.password")}</option>
                <option value="keyPair">{tr("database.dataSourceDialog.keyPair")}</option>
                <option value="agent">{tr("database.dataSourceDialog.opensshConfigAndAuthenticationAgent")}</option>
              </select>
            </Field>
            <Show when={ssh().auth === "keyPair"}>
              <FileField label={tr("database.dataSourceDialog.privateKeyFile")} value={ssh().keyFile ?? ""} testid="dsd-ssh-key" set={(v) => setSsh((t) => (t.keyFile = v || null))} />
            </Show>
            <Show when={ssh().auth !== "agent"}>
              <Field label={secretLabel()}>
                <input
                  class="dsd-input"
                  type="password"
                  disabled={!ssh().enabled}
                  placeholder={p.d.hadSshSecret && p.d.sshSecret === undefined ? tr("tools.database.hidden") : ""}
                  value={p.d.sshSecret ?? ""}
                  onInput={(e) => setSecret("sshSecret", e.currentTarget.value)}
                />
              </Field>
            </Show>
          </Show>
          <Show when={!adhoc()}>
            <div class="dsd-hint muted">{tr("tools.database.hostUserAndAuthenticationCome")}</div>
          </Show>
        </div>
        <div class="dsd-section">{tr("database.dataSourceDialog.ssl")}</div>
        <Field label={tr("database.dataSourceDialog.mode")}>
          <select class="dsd-input dsd-narrow" value={p.d.source.options.sslMode} data-testid="dsd-ssl-mode" onChange={(e) => update((s) => (s.options.sslMode = e.currentTarget.value as SslMode))}>
            <option value="disable">{tr("database.dataSourceDialog.disable")}</option>
            <option value="prefer">{tr("database.dataSourceDialog.prefer")}</option>
            <option value="require">{tr("database.dataSourceDialog.require")}</option>
            <option value="verify-ca">{tr("database.dataSourceDialog.verifyCa")}</option>
            <option value="verify-full">{tr("database.dataSourceDialog.verifyFull")}</option>
          </select>
        </Field>
        <div class="dsd-indent" classList={{ disabled: p.d.source.options.sslMode === "disable" }}>
          <FileField label={tr("database.dataSourceDialog.caFile")} value={ssl().caFile ?? ""} testid="dsd-ssl-ca" set={(v) => setSsl((f) => (f.caFile = v))} />
          <FileField label={tr("database.dataSourceDialog.clientCertificateFile")} value={ssl().certFile ?? ""} testid="dsd-ssl-cert" set={(v) => setSsl((f) => (f.certFile = v))} />
          <FileField label={tr("database.dataSourceDialog.clientKeyFile")} value={ssl().keyFile ?? ""} testid="dsd-ssl-key" set={(v) => setSsl((f) => (f.keyFile = v))} />
          <Field label={tr("database.dataSourceDialog.clientKeyPassword")}>
            <input
              class="dsd-input"
              type="password"
              placeholder={p.d.hadSslKey && p.d.sslKey === undefined ? tr("tools.database.hidden") : ""}
              value={p.d.sslKey ?? ""}
              onInput={(e) => setSecret("sslKey", e.currentTarget.value)}
            />
          </Field>
        </div>
        <Show when={SSL_NOTES[p.d.source.driver]}>{(n) => <div class="dsd-hint muted">{n()}</div>}</Show>
        <div class="dsd-hint muted">{tr("database.dataSourceDialog.relativeCertificatePathsAreResolved")}</div>
      </div>
    );
  }

  function OptionsPage(p: { d: Draft }) {
    const d = () => p.d;
    const transactions = () => ["mysql", "mariadb", "postgres", "sqlite", "sqlserver"].includes(d().source.driver);
    return (
      <>
        <div class="dsd-section">{tr("database.dataSourceDialog.connection")}</div>
        <label class="dsd-check">
          <input type="checkbox" checked={d().source.options.readOnly} data-testid="dsd-read-only" onChange={(e) => update((s) => (s.options.readOnly = e.currentTarget.checked))} />
          {tr("database.dataSourceDialog.readOnly")}
        </label>
        <Field label={tr("database.dataSourceDialog.color")}>
          <div class="dsd-row">
            <span class="dsd-swatch" style={{ background: DS_COLORS[d().source.options.color ?? ""]?.accent ?? "transparent" }} />
            <select
              class="dsd-input dsd-narrow"
              value={d().source.options.color ?? ""}
              data-testid="dsd-color"
              onChange={(e) => update((s) => (s.options.color = e.currentTarget.value || null))}
            >
              <option value="">{tr("database.dataSourceDialog.none")}</option>
              <For each={Object.entries(DS_COLORS)}>{([id, c]) => <option value={id}>{c.label}</option>}</For>
            </select>
          </div>
        </Field>
        <Show when={d().source.options.color === "red"}>
          <div class="dsd-hint muted">{tr("tools.database.productionConsolesAskBeforeUpdate")}{})).</div>
        </Show>
        <Show when={transactions()}>
          <Field label={tr("database.dataSourceDialog.transactionControl")}>
            <select
              class="dsd-input dsd-narrow"
              value={d().source.options.autoCommit ? "auto" : "manual"}
              onChange={(e) => update((s) => (s.options.autoCommit = e.currentTarget.value === "auto"))}
            >
              <option value="auto">{tr("database.dataSourceDialog.auto")}</option>
              <option value="manual">{tr("database.dataSourceDialog.manual")}</option>
            </select>
          </Field>
        </Show>
        <label class="dsd-check">
          <input type="checkbox" checked={!!d().source.options.singleSession} onChange={(e) => update((s) => (s.options.singleSession = e.currentTarget.checked))} />
          {tr("tools.database.singleSessionModeAllConsoles")}
        </label>
        <Field label={tr("database.dataSourceDialog.connectTimeoutS")}>
          <input
            class="dsd-input dsd-port"
            value={d().source.options.connectTimeoutSecs}
            onInput={(e) => update((s) => (s.options.connectTimeoutSecs = Math.max(1, Number(e.currentTarget.value) || 10)))}
          />
        </Field>
        <Show when={d().source.driver !== "redis" && d().source.driver !== "mongodb"}>
          <Field label={tr("database.dataSourceDialog.startupScript")}>
            <textarea
              class="dsd-input dsd-textarea"
              spellcheck={false}
              placeholder={tr("tools.database.setTimeZone0000")}
              value={d().source.options.initSql}
              onInput={(e) => update((s) => (s.options.initSql = e.currentTarget.value))}
            />
          </Field>
        </Show>
        <div class="dsd-section">{tr("database.dataSourceDialog.dataViews")}</div>
        <Field label={tr("database.dataSourceDialog.pageSize")}>
          <input
            class="dsd-input dsd-port"
            placeholder="500"
            value={d().source.options.pageSize ?? ""}
            onInput={(e) => update((s) => (s.options.pageSize = Number(e.currentTarget.value) || null))}
          />
        </Field>
        <div class="dsd-section">{tr("database.dataSourceDialog.scope")}</div>
        <label class="dsd-check">
          <input type="checkbox" checked={d().source.scope === "global"} onChange={(e) => update((s) => (s.scope = e.currentTarget.checked ? "global" : "project"))} />
          {tr("tools.database.globalAvailableInAllProjects")}
        </label>
      </>
    );
  }
}
