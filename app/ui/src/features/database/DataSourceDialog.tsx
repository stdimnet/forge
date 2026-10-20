/**
 * Data Sources and Drivers dialog: list of project/global data sources on the left, the
 * selected one's General and Options tabs on the right, Test Connection, OK/Cancel/Apply.
 */
import { createMemo, createSignal, For, Show } from "solid-js";
import { createStore, produce } from "solid-js/store";
import { CircleCheck, CircleX, Copy, Database, LoaderCircle, Minus, Plus, X } from "lucide-solid";
import {
  dbApi,
  defaultOptions,
  DEFAULT_PORTS,
  DRIVER_LABELS,
  type DataSource,
  type DataSourceInfo,
  type Driver,
  type TestResult,
} from "@/ipc/db";
import { isTauri } from "@/ipc";
import { workspace } from "@/state/workspace";
import { showBalloon } from "@/keymap";
import { ContextMenu, type MenuItem } from "./ContextMenu";
import { dialog, refresh, setDialog, sources, states } from "./store";

interface Draft {
  key: string;
  source: DataSource;
  /** `undefined` keeps the stored password. */
  password?: string;
  hadPassword: boolean;
  dirty: boolean;
  isNew: boolean;
}

const DRIVERS: Driver[] = ["mysql", "mariadb", "postgres", "sqlite"];

/** JDBC-style URL shown under the fields, like DataGrip. */
export function connectionUrl(ds: DataSource): string {
  if (ds.driver === "sqlite") return `jdbc:sqlite:${ds.file}`;
  const scheme = ds.driver === "postgres" ? "postgresql" : ds.driver;
  const port = ds.port ?? DEFAULT_PORTS[ds.driver];
  return `jdbc:${scheme}://${ds.host || "localhost"}${port ? `:${port}` : ""}${ds.database ? `/${ds.database}` : ""}`;
}

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
    database: "",
    user: driver === "postgres" ? "postgres" : driver === "sqlite" ? "" : "root",
    file: driver === "sqlite" ? "database/database.sqlite" : "",
    comment: "",
    scope: "project",
    options: defaultOptions(),
  };
}

const fromInfo = (i: DataSourceInfo): DataSource => {
  const { hasPassword: _h, summary: _s, via: _v, ...ds } = i;
  return structuredClone(ds);
};

function Field(props: { label: string; children: unknown; wide?: boolean }) {
  return (
    <label class="dsd-field" classList={{ wide: props.wide }}>
      <span class="dsd-label">{props.label}:</span>
      {props.children as never}
    </label>
  );
}

export function DataSourceDialog() {
  return (
    <Show when={dialog()}>
      {(req) => <DialogBody select={req().select} newDriver={req().newDriver} />}
    </Show>
  );
}

function DialogBody(props: { select?: string; newDriver?: Driver }) {
  let seq = 0;
  const initial: Draft[] = sources().map((s) => ({ key: s.id, source: fromInfo(s), hadPassword: s.hasPassword, dirty: false, isNew: false }));
  const [drafts, setDrafts] = createStore<Draft[]>(initial);
  const [deleted, setDeleted] = createSignal<string[]>([]);
  const [selected, setSelected] = createSignal<string | null>(props.select ?? initial[0]?.key ?? null);
  const [tab, setTab] = createSignal<"general" | "options">("general");
  const [test, setTest] = createSignal<TestResult | "running" | null>(null);
  const [menu, setMenu] = createSignal<{ x: number; y: number; items: MenuItem[] } | null>(null);
  const [saving, setSaving] = createSignal(false);

  const addNew = (driver: Driver, base?: DataSource) => {
    const key = `new-${++seq}`;
    const source = base ? { ...structuredClone(base), id: "", name: `${base.name} (copy)` } : blank(driver);
    setDrafts((d) => [...d, { key, source, hadPassword: false, dirty: true, isNew: true, password: base ? undefined : "" }]);
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

  const remove = () => {
    const d = cur();
    if (!d) return;
    if (!d.isNew) setDeleted((x) => [...x, d.source.id]);
    const i = index();
    setDrafts((list) => list.filter((x) => x.key !== d.key));
    setSelected(drafts[Math.min(i, drafts.length - 1)]?.key ?? null);
  };

  const runTest = async () => {
    const d = cur();
    if (!d) return;
    setTest("running");
    try {
      setTest(await dbApi.test(d.source, d.password));
    } catch (e) {
      setTest({ ok: false, driver: DRIVER_LABELS[d.source.driver], latencyMs: 0, error: e instanceof Error ? e.message : String(e) });
    }
  };

  const apply = async (): Promise<boolean> => {
    setSaving(true);
    try {
      for (const id of deleted()) await dbApi.remove(id);
      setDeleted([]);
      for (let i = 0; i < drafts.length; i++) {
        const d = drafts[i];
        if (!d.dirty) continue;
        const saved = await dbApi.save(d.source, d.password);
        setDrafts(i, { key: saved.id, source: fromInfo(saved), hadPassword: saved.hasPassword, dirty: false, isNew: false, password: undefined });
        if (selected() === d.key) setSelected(saved.id);
        if (states[saved.id]) void refresh(saved.id);
      }
      return true;
    } catch (e) {
      showBalloon("Cannot save data source", e instanceof Error ? e.message : String(e));
      return false;
    } finally {
      setSaving(false);
    }
  };

  const close = () => setDialog(null);
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
    <div class="dsd-overlay" onMouseDown={(e) => e.target === e.currentTarget && close()}>
      <div
        class="dsd-dialog"
        role="dialog"
        aria-label="Data Sources and Drivers"
        onKeyDown={(e) => {
          if (e.key === "Escape" && !menu()) {
            e.stopPropagation();
            close();
          }
        }}
      >
        <div class="dsd-titlebar">
          <span>Data Sources and Drivers</span>
          <button class="icon-btn" title="Close" onClick={close}>
            <X size={15} />
          </button>
        </div>
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
                        <Show when={d().source.scope === "project" && workspace()?.remote}>
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
                        <Field label="Database">
                          <input class="dsd-input" value={d().source.database} spellcheck={false} onInput={(e) => update((s) => (s.database = e.currentTarget.value))} />
                        </Field>
                      </Show>
                      <Field label="URL">
                        <input class="dsd-input dsd-url" value={connectionUrl(d().source)} readOnly />
                      </Field>
                      <div class="dsd-hint muted">Passwords are stored in the system keychain, never in project files.</div>
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
                                    <CircleX size={15} />
                                    <span>
                                      <b>Failed</b> <span class="dsd-test-error">{r.error}</span>
                                    </span>
                                  </span>
                                );
                              })()}
                            </Show>
                          )}
                        </Show>
                      </div>
                    </Show>
                    <Show when={tab() === "options"}>
                      <div class="dsd-section">Connection</div>
                      <label class="dsd-check">
                        <input type="checkbox" checked={d().source.options.readOnly} onChange={(e) => update((s) => (s.options.readOnly = e.currentTarget.checked))} />
                        Read-only
                      </label>
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
                      <Show when={d().source.driver !== "sqlite"}>
                        <Field label="SSL mode">
                          <select class="dsd-input dsd-narrow" value={d().source.options.sslMode} onChange={(e) => update((s) => (s.options.sslMode = e.currentTarget.value as DataSource["options"]["sslMode"]))}>
                            <option value="disable">Disable</option>
                            <option value="prefer">Prefer</option>
                            <option value="require">Require</option>
                          </select>
                        </Field>
                      </Show>
                      <Field label="Connect timeout (s)">
                        <input
                          class="dsd-input dsd-port"
                          value={d().source.options.connectTimeoutSecs}
                          onInput={(e) => update((s) => (s.options.connectTimeoutSecs = Math.max(1, Number(e.currentTarget.value) || 10)))}
                        />
                      </Field>
                      <Field label="Startup script">
                        <textarea
                          class="dsd-input dsd-textarea"
                          spellcheck={false}
                          placeholder="SET time_zone = '+00:00';"
                          value={d().source.options.initSql}
                          onInput={(e) => update((s) => (s.options.initSql = e.currentTarget.value))}
                        />
                      </Field>
                      <div class="dsd-section">Data Views</div>
                      <Field label="Page size">
                        <input
                          class="dsd-input dsd-port"
                          placeholder="500"
                          value={d().source.options.pageSize ?? ""}
                          onInput={(e) => update((s) => (s.options.pageSize = Number(e.currentTarget.value) || null))}
                        />
                      </Field>
                      <Field label="Schemas">
                        <input
                          class="dsd-input"
                          placeholder="current schema; * for all"
                          value={d().source.options.schemas.join(", ")}
                          onInput={(e) =>
                            update((s) => (s.options.schemas = e.currentTarget.value.split(",").map((x) => x.trim()).filter(Boolean)))
                          }
                        />
                      </Field>
                      <div class="dsd-section">Scope</div>
                      <label class="dsd-check">
                        <input
                          type="checkbox"
                          checked={d().source.scope === "global"}
                          onChange={(e) => update((s) => (s.scope = e.currentTarget.checked ? "global" : "project"))}
                        />
                        Global (available in all projects)
                      </label>
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
    </div>
  );
}
