/**
 * Create Table and Modify Table (DataGrip): the table's structure as a tree (columns, keys,
 * foreign keys, indexes, checks) with a property form for the selected node, table options
 * per dialect, and a live SQL preview generated in Rust (`CREATE TABLE`, or the ALTER
 * statements of a rename-aware diff with warnings for destructive changes). OK executes the
 * script (read-only refused, production confirmed) or opens it in a console.
 */
import { createEffect, createMemo, createResource, createSignal, For, on, onCleanup, Show } from "solid-js";
import { createStore, produce, reconcile, unwrap } from "solid-js/store";
import { ArrowDown, ArrowUp, Columns3, KeyRound, Link2, ListTree, Minus, Plus, ShieldCheck, Table2, TriangleAlert } from "lucide-solid";
import { sourceDialect } from "@/ipc/db";
import { dbObjectsApi, emptyTable, type ColumnSpec, type DesignPreview, type TableSpec } from "@/ipc/dbObjects";
import { showBalloon } from "@/keymap";
import { ContextMenu, type MenuItem } from "../ContextMenu";
import { sourceById, visibleSchemas } from "../store";
import { ExecutionMode, ObjectModal } from "./Modal";
import { SqlPreview } from "./SqlPreview";
import { closeObjectDialog, loadedTables, runObjectScript, scriptToConsole } from "./state";

type Section = "table" | "columns" | "keys" | "fks" | "indexes" | "checks";
interface Sel {
  section: Section;
  index: number;
}

const SECTION_LABELS: Record<Exclude<Section, "table">, string> = {
  columns: "columns",
  keys: "keys",
  fks: "foreign keys",
  indexes: "indexes",
  checks: "checks",
};

export function TableDialog(p: { ds: string; schema: string; table?: string }) {
  const ds = sourceById(p.ds);
  const dialect = sourceDialect(ds);
  const modify = !!p.table;
  const [info] = createResource(() => dbObjectsApi.designInfo(dialect));
  const [spec, setSpec] = createStore<TableSpec>(emptyTable(p.schema));
  const [old, setOld] = createSignal<TableSpec | null>(null);
  const [loaded, setLoaded] = createSignal(!modify);
  const [sel, setSel] = createSignal<Sel>({ section: modify ? "table" : "columns", index: 0 });
  const [mode, setMode] = createSignal<"execute" | "console">("execute");
  const [preview, setPreview] = createSignal<DesignPreview | null>(null);
  const [addMenu, setAddMenu] = createSignal<{ x: number; y: number } | null>(null);
  const [busy, setBusy] = createSignal(false);

  if (modify) {
    dbObjectsApi
      .designLoad(p.ds, p.schema, p.table!)
      .then((t) => {
        setOld(structuredClone(t));
        setSpec(reconcile(t));
        setLoaded(true);
      })
      .catch((e) => {
        showBalloon("Cannot modify table", e instanceof Error ? e.message : String(e));
        closeObjectDialog();
      });
  } else {
    createEffect(
      on(info, (i) => {
        if (!i || spec.columns.length) return;
        // DataGrip starts a new table with an id column.
        const idType = { mysql: "bigint", postgres: "bigint", sqlite: "INTEGER", sqlserver: "bigint", oracle: "NUMBER(19)", clickhouse: "UInt64" }[dialect as string] ?? "bigint";
        setSpec("columns", [{ name: "id", dataType: idType, nullable: false, comment: "", autoIncrement: i.autoIncrement }]);
        if (i.indexes) setSpec("primaryKey", { name: "", columns: ["id"] });
      }),
    );
  }

  // The live preview (debounced).
  const snapshot = createMemo(() => JSON.stringify(spec));
  let timer: ReturnType<typeof setTimeout> | undefined;
  onCleanup(() => clearTimeout(timer));
  createEffect(
    on([snapshot, loaded], ([, ok]) => {
      if (!ok) return;
      clearTimeout(timer);
      timer = setTimeout(() => {
        const t = unwrap(spec);
        dbObjectsApi
          .designPreview(dialect, structuredClone(t), old() ? structuredClone(old()!) : null)
          .then(setPreview)
          .catch((e) => setPreview({ sql: `-- ${e instanceof Error ? e.message : String(e)}`, statements: [], warnings: [] }));
      }, 120);
    }),
  );

  const keys = () => [...(spec.primaryKey ? [{ ...spec.primaryKey, primary: true }] : []), ...spec.uniqueKeys.map((k) => ({ ...k, primary: false }))];
  const columnNames = () => spec.columns.map((c) => c.name).filter(Boolean);
  const isPk = (name: string) => !!spec.primaryKey?.columns.includes(name);

  const uniqueName = (base: string, taken: string[]) => {
    let n = base;
    for (let i = 2; taken.includes(n); i++) n = `${base}${i}`;
    return n;
  };

  const add = (section: Section) => {
    const t = spec.name || "table";
    const cols = columnNames();
    switch (section) {
      case "columns":
        setSpec("columns", (c) => [...c, { name: uniqueName("column", cols), dataType: info()?.defaultType ?? "varchar(255)", nullable: true, comment: "", autoIncrement: false }]);
        setSel({ section, index: spec.columns.length - 1 });
        break;
      case "keys":
        if (!spec.primaryKey) setSpec("primaryKey", { name: `${t}_pk`, columns: cols.slice(0, 1) });
        else setSpec("uniqueKeys", (k) => [...k, { name: uniqueName(`${t}_uk`, keys().map((x) => x.name)), columns: [] }]);
        setSel({ section, index: keys().length - 1 });
        break;
      case "fks": {
        const target = loadedTables(p.ds).find((x) => x.name !== spec.name);
        setSpec("foreignKeys", (f) => [
          ...f,
          {
            name: uniqueName(`${t}_${target?.name ?? "ref"}_fk`, f.map((x) => x.name)),
            columns: [],
            refSchema: target?.schema ?? p.schema,
            refTable: target?.name ?? "",
            refColumns: target?.primaryKey.slice(0, 1) ?? [],
            onUpdate: "",
            onDelete: "",
          },
        ]);
        setSel({ section, index: spec.foreignKeys.length - 1 });
        break;
      }
      case "indexes":
        setSpec("indexes", (i) => [...i, { name: uniqueName(`${t}_idx`, i.map((x) => x.name)), columns: cols.slice(-1), unique: false, method: "" }]);
        setSel({ section, index: spec.indexes.length - 1 });
        break;
      case "checks":
        setSpec("checks", (c) => [...c, { name: uniqueName(`${t}_check`, c.map((x) => x.name)), expression: "" }]);
        setSel({ section, index: spec.checks.length - 1 });
        break;
    }
  };

  const remove = () => {
    const s = sel();
    switch (s.section) {
      case "columns": {
        const name = spec.columns[s.index]?.name;
        setSpec(
          produce((t) => {
            t.columns.splice(s.index, 1);
            if (t.primaryKey) {
              t.primaryKey.columns = t.primaryKey.columns.filter((c) => c !== name);
              if (!t.primaryKey.columns.length) t.primaryKey = null;
            }
          }),
        );
        break;
      }
      case "keys":
        if (spec.primaryKey && s.index === 0) setSpec("primaryKey", null);
        else setSpec("uniqueKeys", (k) => k.filter((_, i) => i !== s.index - (spec.primaryKey ? 1 : 0)));
        break;
      case "fks":
        setSpec("foreignKeys", (f) => f.filter((_, i) => i !== s.index));
        break;
      case "indexes":
        setSpec("indexes", (f) => f.filter((_, i) => i !== s.index));
        break;
      case "checks":
        setSpec("checks", (f) => f.filter((_, i) => i !== s.index));
        break;
      default:
        return;
    }
    setSel({ section: s.section, index: Math.max(0, s.index - 1) });
  };

  const move = (delta: number) => {
    const s = sel();
    if (s.section !== "columns") return;
    const to = s.index + delta;
    if (to < 0 || to >= spec.columns.length) return;
    setSpec(
      "columns",
      produce((c) => {
        const [x] = c.splice(s.index, 1);
        c.splice(to, 0, x);
      }),
    );
    setSel({ section: "columns", index: to });
  };

  const renameColumn = (i: number, name: string) => {
    const before = spec.columns[i].name;
    setSpec(
      produce((t) => {
        t.columns[i].name = name;
        const fix = (cols: string[]) => cols.map((c) => (c === before ? name : c));
        if (t.primaryKey) t.primaryKey.columns = fix(t.primaryKey.columns);
        t.uniqueKeys.forEach((k) => (k.columns = fix(k.columns)));
        t.foreignKeys.forEach((k) => (k.columns = fix(k.columns)));
        t.indexes.forEach((k) => (k.columns = fix(k.columns)));
      }),
    );
  };

  const togglePk = (name: string, on: boolean) =>
    setSpec(
      produce((t) => {
        const cols = new Set(t.primaryKey?.columns ?? []);
        if (on) cols.add(name);
        else cols.delete(name);
        t.primaryKey = cols.size ? { name: t.primaryKey?.name ?? "", columns: t.columns.map((c) => c.name).filter((c) => cols.has(c)) } : null;
        if (on) {
          const c = t.columns.find((x) => x.name === name);
          if (c) c.nullable = false;
        }
      }),
    );

  const addItems = (): MenuItem[] => {
    const i = info();
    return [
      { label: "Column", icon: <Columns3 size={13} />, run: () => add("columns") },
      { label: spec.primaryKey ? "Unique Key" : "Primary Key", icon: <KeyRound size={13} />, disabled: !i?.indexes, run: () => add("keys") },
      { label: "Foreign Key", icon: <Link2 size={13} />, disabled: !i?.foreignKeys, run: () => add("fks") },
      { label: "Index", icon: <ListTree size={13} />, disabled: !i?.indexes, run: () => add("indexes") },
      { label: "Check", icon: <ShieldCheck size={13} />, disabled: !i?.checks, run: () => add("checks") },
    ];
  };

  const sqlText = () => preview()?.sql ?? "";
  const nothing = () => modify && loaded() && preview() !== null && !preview()!.statements.length;
  const title = modify ? `Modify Table ${p.table}` : "Create Table";

  const ok = async () => {
    const sql = sqlText();
    if (!sql.trim()) return closeObjectDialog();
    if (mode() === "console") {
      scriptToConsole(p.ds, sql);
      closeObjectDialog();
      return;
    }
    setBusy(true);
    const done = await runObjectScript({ ds: p.ds, schema: spec.schema, sql, title: modify ? `Modify table ${spec.name}` : `Create table ${spec.name}`, warnings: preview()?.warnings });
    setBusy(false);
    if (done) closeObjectDialog();
  };

  const node = (section: Section, index: number, icon: () => any, label: string, hint?: string) => (
    <div
      class="dbo-node"
      classList={{ selected: sel().section === section && sel().index === index, child: section !== "table" }}
      onClick={() => setSel({ section, index })}
      data-testid={`dbo-node-${section}-${index}`}
    >
      {icon()}
      <span class="dbo-node-label">{label || "<unnamed>"}</span>
      <Show when={hint}>
        <span class="dbo-node-hint">{hint}</span>
      </Show>
    </div>
  );

  const group = (section: Exclude<Section, "table">, count: number) => (
    <div class="dbo-group" onClick={() => count === 0 && add(section)}>
      {SECTION_LABELS[section]} <span class="dbo-node-hint">{count}</span>
    </div>
  );

  return (
    <ObjectModal
      title={title}
      testId="dbo-table-dialog"
      width={980}
      height={700}
      onClose={closeObjectDialog}
      footerStart={<ExecutionMode value={mode()} onChange={setMode} disabled={ds?.options.readOnly} />}
      footer={
        <>
          <button class="btn-secondary" onClick={closeObjectDialog}>
            Cancel
          </button>
          <button class="btn-primary" disabled={busy() || !spec.name.trim() || !spec.columns.length || nothing()} onClick={() => void ok()} data-testid="dbo-table-ok">
            {mode() === "console" ? "Open in Console" : "OK"}
          </button>
        </>
      }
    >
      <div class="dbo-table">
        <div class="dbo-tree-pane">
          <div class="dbo-toolbar">
            <button class="icon-btn" title="Add" onClick={(e) => setAddMenu({ x: e.currentTarget.getBoundingClientRect().left, y: e.currentTarget.getBoundingClientRect().bottom + 2 })} data-testid="dbo-add">
              <Plus size={14} />
            </button>
            <button class="icon-btn" title="Remove" disabled={sel().section === "table"} onClick={remove}>
              <Minus size={14} />
            </button>
            <button class="icon-btn" title="Move Up" disabled={sel().section !== "columns" || sel().index === 0} onClick={() => move(-1)}>
              <ArrowUp size={14} />
            </button>
            <button class="icon-btn" title="Move Down" disabled={sel().section !== "columns" || sel().index >= spec.columns.length - 1} onClick={() => move(1)}>
              <ArrowDown size={14} />
            </button>
          </div>
          <div class="dbo-tree">
            {node("table", 0, () => <Table2 size={14} class="db-ic-table" />, spec.name, spec.schema)}
            {group("columns", spec.columns.length)}
            <For each={spec.columns}>
              {(c, i) =>
                node(
                  "columns",
                  i(),
                  () => (isPk(c.name) ? <KeyRound size={13} class="db-ic-key" /> : <Columns3 size={13} class="db-ic-column" />),
                  c.name,
                  `${c.dataType}${c.nullable ? "" : " not null"}`,
                )
              }
            </For>
            <Show when={info()?.indexes || keys().length}>
              {group("keys", keys().length)}
              <For each={keys()}>{(k, i) => node("keys", i(), () => <KeyRound size={13} class="db-ic-key" />, k.name || (k.primary ? "primary key" : "unique key"), `(${k.columns.join(", ")})`)}</For>
            </Show>
            <Show when={info()?.foreignKeys || spec.foreignKeys.length}>
              {group("fks", spec.foreignKeys.length)}
              <For each={spec.foreignKeys}>{(f, i) => node("fks", i(), () => <Link2 size={13} class="db-ic-fk" />, f.name, `→ ${f.refTable}`)}</For>
            </Show>
            <Show when={info()?.indexes || spec.indexes.length}>
              {group("indexes", spec.indexes.length)}
              <For each={spec.indexes}>{(x, i) => node("indexes", i(), () => <ListTree size={13} class="db-ic-index" />, x.name, `(${x.columns.join(", ")})`)}</For>
            </Show>
            {group("checks", spec.checks.length)}
            <For each={spec.checks}>{(c, i) => node("checks", i(), () => <ShieldCheck size={13} class="db-ic-check" />, c.name, c.expression)}</For>
          </div>
        </div>
        <div class="dbo-props" data-testid="dbo-props">
          <datalist id="dbo-types">
            <For each={info()?.types ?? []}>{(t) => <option value={t} />}</For>
          </datalist>
          <Show when={sel().section === "table"}>
            <div class="db-form">
              <Show when={!modify}>
                <span class="db-form-label">Schema:</span>
                <select class="dsd-input" value={spec.schema} onChange={(e) => setSpec("schema", e.currentTarget.value)}>
                  <For each={visibleSchemas(p.ds)}>{(s) => <option value={s.name}>{s.name}</option>}</For>
                </select>
              </Show>
              <span class="db-form-label">Name:</span>
              <input class="dsd-input" value={spec.name} spellcheck={false} data-autofocus onInput={(e) => setSpec("name", e.currentTarget.value)} data-testid="dbo-table-name" />
              <Show when={info()?.comments}>
                <span class="db-form-label">Comment:</span>
                <input class="dsd-input" value={spec.comment} onInput={(e) => setSpec("comment", e.currentTarget.value)} />
              </Show>
              <For each={info()?.options ?? []}>
                {(o) =>
                  o.kind === "bool" ? (
                    <>
                      <span />
                      <label class="dbo-check">
                        <input type="checkbox" checked={!!spec.options[o.key]} onChange={(e) => setSpec("options", o.key, e.currentTarget.checked as never)} />
                        {o.label}
                      </label>
                    </>
                  ) : (
                    <>
                      <span class="db-form-label">{o.label}:</span>
                      <input class="dsd-input" list={`dbo-opt-${o.key}`} value={String(spec.options[o.key] ?? "")} placeholder="default" onInput={(e) => setSpec("options", o.key, e.currentTarget.value as never)} />
                      <datalist id={`dbo-opt-${o.key}`}>
                        <For each={o.choices ?? []}>{(c) => <option value={c} />}</For>
                      </datalist>
                    </>
                  )
                }
              </For>
            </div>
          </Show>
          <Show when={sel().section === "columns" && spec.columns[sel().index]}>
            {(_) => {
              const i = () => sel().index;
              const c = () => spec.columns[i()] as ColumnSpec;
              const set = <K extends keyof ColumnSpec>(k: K, v: ColumnSpec[K]) => setSpec("columns", i(), k, v);
              return (
                <div class="db-form">
                  <span class="db-form-label">Name:</span>
                  <input class="dsd-input" value={c().name} spellcheck={false} onInput={(e) => renameColumn(i(), e.currentTarget.value)} data-testid="dbo-col-name" />
                  <span class="db-form-label">Type:</span>
                  <input class="dsd-input mono" list="dbo-types" value={c().dataType} spellcheck={false} onInput={(e) => set("dataType", e.currentTarget.value)} data-testid="dbo-col-type" />
                  <span class="db-form-label">Default:</span>
                  <input class="dsd-input mono" value={c().default ?? ""} placeholder="no default" spellcheck={false} onInput={(e) => set("default", e.currentTarget.value || null)} />
                  <Show when={info()?.comments}>
                    <span class="db-form-label">Comment:</span>
                    <input class="dsd-input" value={c().comment} onInput={(e) => set("comment", e.currentTarget.value)} />
                  </Show>
                  <span />
                  <div class="dbo-checks">
                    <label class="dbo-check">
                      <input type="checkbox" checked={!c().nullable} onChange={(e) => set("nullable", !e.currentTarget.checked)} />
                      Not null
                    </label>
                    <Show when={info()?.autoIncrement}>
                      <label class="dbo-check">
                        <input type="checkbox" checked={c().autoIncrement} onChange={(e) => set("autoIncrement", e.currentTarget.checked)} />
                        Auto increment
                      </label>
                    </Show>
                    <Show when={info()?.indexes}>
                      <label class="dbo-check">
                        <input type="checkbox" checked={isPk(c().name)} onChange={(e) => togglePk(c().name, e.currentTarget.checked)} />
                        Primary key
                      </label>
                    </Show>
                  </div>
                  <Show when={c().original && c().original !== c().name}>
                    <span />
                    <span class="dsd-hint muted">Renamed from {c().original}</span>
                  </Show>
                </div>
              );
            }}
          </Show>
          <Show when={sel().section === "keys" && keys()[sel().index]}>
            {(k) => {
              const primary = () => k().primary;
              const ui = () => sel().index - (spec.primaryKey ? 1 : 0);
              const setName = (v: string) => (primary() ? setSpec("primaryKey", "name", v) : setSpec("uniqueKeys", ui(), "name", v));
              const setCols = (v: string[]) => (primary() ? setSpec("primaryKey", "columns", v) : setSpec("uniqueKeys", ui(), "columns", v));
              return (
                <div class="db-form">
                  <span class="db-form-label">Kind:</span>
                  <span>{primary() ? "Primary key" : "Unique key"}</span>
                  <span class="db-form-label">Name:</span>
                  <input class="dsd-input" value={k().name} spellcheck={false} onInput={(e) => setName(e.currentTarget.value)} />
                  <span class="db-form-label">Columns:</span>
                  <ColumnPicker all={columnNames()} value={k().columns} onChange={setCols} />
                </div>
              );
            }}
          </Show>
          <Show when={sel().section === "fks" && spec.foreignKeys[sel().index]}>
            {(_) => {
              const i = () => sel().index;
              const f = () => spec.foreignKeys[i()];
              const targets = () => loadedTables(p.ds);
              const target = () => targets().find((t) => t.schema === f().refSchema && t.name === f().refTable);
              return (
                <div class="db-form">
                  <span class="db-form-label">Name:</span>
                  <input class="dsd-input" value={f().name} spellcheck={false} onInput={(e) => setSpec("foreignKeys", i(), "name", e.currentTarget.value)} />
                  <span class="db-form-label">Columns:</span>
                  <ColumnPicker all={columnNames()} value={f().columns} onChange={(v) => setSpec("foreignKeys", i(), "columns", v)} />
                  <span class="db-form-label">Target table:</span>
                  <select
                    class="dsd-input"
                    value={`${f().refSchema}.${f().refTable}`}
                    data-testid="dbo-fk-target"
                    onChange={(e) => {
                      const t = targets().find((x) => `${x.schema}.${x.name}` === e.currentTarget.value);
                      if (!t) return;
                      setSpec("foreignKeys", i(), { refSchema: t.schema, refTable: t.name, refColumns: t.primaryKey.slice(0, Math.max(1, f().columns.length)) });
                    }}
                  >
                    <For each={targets()}>{(t) => <option value={`${t.schema}.${t.name}`}>{`${t.schema}.${t.name}`}</option>}</For>
                  </select>
                  <span class="db-form-label">Target columns:</span>
                  <ColumnPicker all={target()?.columns ?? []} value={f().refColumns} onChange={(v) => setSpec("foreignKeys", i(), "refColumns", v)} />
                  <span class="db-form-label">On update:</span>
                  <select class="dsd-input" value={f().onUpdate || "NO ACTION"} onChange={(e) => setSpec("foreignKeys", i(), "onUpdate", e.currentTarget.value)}>
                    <For each={info()?.fkActions ?? []}>{(a) => <option>{a}</option>}</For>
                  </select>
                  <span class="db-form-label">On delete:</span>
                  <select class="dsd-input" value={f().onDelete || "NO ACTION"} onChange={(e) => setSpec("foreignKeys", i(), "onDelete", e.currentTarget.value)}>
                    <For each={info()?.fkActions ?? []}>{(a) => <option>{a}</option>}</For>
                  </select>
                </div>
              );
            }}
          </Show>
          <Show when={sel().section === "indexes" && spec.indexes[sel().index]}>
            {(_) => {
              const i = () => sel().index;
              const x = () => spec.indexes[i()];
              return (
                <div class="db-form">
                  <span class="db-form-label">Name:</span>
                  <input class="dsd-input" value={x().name} spellcheck={false} onInput={(e) => setSpec("indexes", i(), "name", e.currentTarget.value)} />
                  <span class="db-form-label">Columns:</span>
                  <ColumnPicker all={columnNames()} value={x().columns} onChange={(v) => setSpec("indexes", i(), "columns", v)} />
                  <Show when={info()?.indexMethods.length}>
                    <span class="db-form-label">Method:</span>
                    <select class="dsd-input" value={x().method} onChange={(e) => setSpec("indexes", i(), "method", e.currentTarget.value)}>
                      <option value="">default</option>
                      <For each={info()?.indexMethods ?? []}>{(m) => <option>{m}</option>}</For>
                    </select>
                  </Show>
                  <span />
                  <label class="dbo-check">
                    <input type="checkbox" checked={x().unique} onChange={(e) => setSpec("indexes", i(), "unique", e.currentTarget.checked)} />
                    Unique
                  </label>
                </div>
              );
            }}
          </Show>
          <Show when={sel().section === "checks" && spec.checks[sel().index]}>
            {(_) => {
              const i = () => sel().index;
              return (
                <div class="db-form">
                  <span class="db-form-label">Name:</span>
                  <input class="dsd-input" value={spec.checks[i()].name} spellcheck={false} onInput={(e) => setSpec("checks", i(), "name", e.currentTarget.value)} />
                  <span class="db-form-label">Expression:</span>
                  <input class="dsd-input mono" value={spec.checks[i()].expression} placeholder="price >= 0" spellcheck={false} onInput={(e) => setSpec("checks", i(), "expression", e.currentTarget.value)} />
                </div>
              );
            }}
          </Show>
        </div>
      </div>
      <Show when={preview()?.warnings.length}>
        <div class="dbo-warnings" data-testid="dbo-warnings">
          <For each={preview()!.warnings}>
            {(w) => (
              <div classList={{ destructive: w.destructive }}>
                <TriangleAlert size={13} /> {w.message}
              </div>
            )}
          </For>
        </div>
      </Show>
      <div class="dbo-preview-label">{modify ? "ALTER script" : "SQL preview"}</div>
      <SqlPreview sql={sqlText()} dialect={dialect} placeholder={modify && loaded() ? "-- No changes" : "-- Loading…"} testId="dbo-table-sql" />
      <Show when={addMenu()}>{(m) => <ContextMenu {...m()} items={addItems()} onClose={() => setAddMenu(null)} />}</Show>
    </ObjectModal>
  );
}

/** Columns of a key or index: chosen with checkboxes, in table order. */
function ColumnPicker(props: { all: string[]; value: string[]; onChange: (v: string[]) => void }) {
  return (
    <div class="dbo-picker">
      <For each={props.all} fallback={<span class="muted">No columns</span>}>
        {(c) => (
          <label class="dbo-check">
            <input
              type="checkbox"
              checked={props.value.includes(c)}
              onChange={(e) => {
                const on = e.currentTarget.checked;
                const keep = props.value.filter((x) => x !== c);
                props.onChange(on ? [...keep, c] : keep);
              }}
            />
            {c}
          </label>
        )}
      </For>
    </div>
  );
}
