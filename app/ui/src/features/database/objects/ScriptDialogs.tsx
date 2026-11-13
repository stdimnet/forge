/**
 * Drop and Rename from the Database tool window (with a SQL preview, cascade for Drop), and
 * the SQL Generator for "SQL Scripts › Generate DDL": objects in dependency order with
 * options (DROP statements, IF NOT EXISTS, qualified names), formatted with the code style.
 */
import { createEffect, createMemo, createSignal, For, on, Show } from "solid-js";
import { Copy, FileDown, SquareTerminal } from "lucide-solid";
import { isTauri } from "@/ipc";
import { sourceDialect } from "@/ipc/db";
import { dbObjectsApi, type DdlObject, type DdlOptions, type DropRequest } from "@/ipc/dbObjects";
import { showBalloon } from "@/keymap";
import { sourceById, states } from "../store";
import { ExecutionMode, ObjectModal } from "./Modal";
import { SqlPreview } from "./SqlPreview";
import { closeObjectDialog, runObjectScript, scriptToConsole } from "./state";
import { t as tr } from "@/i18n";
import { writeClipboard, writeClipboardText } from "@/features/clipboard/system";

const text = (e: unknown) => (e instanceof Error ? e.message : String(e));

export function DropDialog(p: { ds: string; schema: string; objects: { kind: string; name: string; table?: string }[] }) {
  const ds = sourceById(p.ds);
  const dialect = sourceDialect(ds);
  const [cascade, setCascade] = createSignal(false);
  const [mode, setMode] = createSignal<"execute" | "console">("execute");
  const [sql, setSql] = createSignal("");
  const requests = (): DropRequest[] =>
    p.objects.map((o) => ({
      schema: p.schema,
      name: o.name,
      kind: o.kind,
      table: o.table,
      cascade: cascade(),
      // Foreign keys of other tables that point here (dropped first where there is no CASCADE).
      referencing: Object.values(states[p.ds]?.introspected ?? {}).flatMap((s) =>
        s.tables.flatMap((t) =>
          t.foreignKeys
            .filter((f) => !f.virtual && f.refTable === o.name && (f.refSchema || s.name) === p.schema)
            .map((f) => ({ schema: s.name, table: t.name, constraint: f.name })),
        ),
      ),
    }));
  createEffect(
    on(cascade, () => {
      dbObjectsApi
        .dropSql(dialect, requests())
        .then(setSql)
        .catch((e) => setSql(`-- ${text(e)}`));
    }),
  );
  const what = p.objects.length === 1 ? `${p.objects[0].kind === "matview" ? "materialized view" : p.objects[0].kind} ${p.objects[0].name}` : `${p.objects.length} objects`;
  const ok = async () => {
    if (mode() === "console") {
      scriptToConsole(p.ds, sql());
      return closeObjectDialog();
    }
    const done = await runObjectScript({
      ds: p.ds,
      schema: p.schema,
      sql: sql(),
      title: tr("database.objects.scriptDialogs.dropWhat", { what }),
      warnings: [{ message: `Dropping ${what} deletes ${p.objects.some((o) => o.kind === "table") ? "its data" : "it"} for good`, destructive: true }],
    });
    if (done) closeObjectDialog();
  };
  return (
    <ObjectModal
      title={tr("database.objects.scriptDialogs.dropWhat", { what })}
      testId="dbo-drop-dialog"
      width={620}
      onClose={closeObjectDialog}
      footerStart={<ExecutionMode value={mode()} onChange={setMode} disabled={ds?.options.readOnly} />}
      footer={
        <>
          <button class="btn-secondary" onClick={closeObjectDialog}>
            {tr("common.cancel")}
          </button>
          <button class="btn-primary danger" disabled={!sql()} onClick={() => void ok()} data-testid="dbo-drop-ok">
            {mode() === "console" ? tr("database.objects.scriptDialogs.openInConsole") : tr("database.objects.scriptDialogs.drop")}
          </button>
        </>
      }
    >
      <div class="dbo-pad">
        <label class="dbo-check">
          <input type="checkbox" checked={cascade()} onChange={(e) => setCascade(e.currentTarget.checked)} data-testid="dbo-cascade" />
          {tr("tools.database.cascadeAlsoDropTheObjects", { count: p.objects.length })}
        </label>
      </div>
      <SqlPreview sql={sql()} dialect={dialect} />
    </ObjectModal>
  );
}

export function RenameDialog(p: { ds: string; schema: string; table: string; column?: string }) {
  const ds = sourceById(p.ds);
  const dialect = sourceDialect(ds);
  const from = p.column ?? p.table;
  const [name, setName] = createSignal(from);
  const [sql, setSql] = createSignal("");
  createEffect(
    on(name, (to) => {
      if (!to.trim() || to === from) return setSql("");
      dbObjectsApi
        .renameSql(dialect, p.schema, p.table, to.trim(), p.column)
        .then(setSql)
        .catch((e) => setSql(`-- ${text(e)}`));
    }),
  );
  const ok = async () => {
    if (!sql()) return;
    const done = await runObjectScript({ ds: p.ds, schema: p.schema, sql: sql(), title: `Rename ${p.column ? "column" : "table"} ${from}` });
    if (done) closeObjectDialog();
  };
  return (
    <ObjectModal
      title={`Rename ${p.column ? tr("database.objects.scriptDialogs.column") : tr("database.objects.scriptDialogs.table")}`}
      testId="dbo-rename-dialog"
      width={560}
      onClose={closeObjectDialog}
      footer={
        <>
          <button class="btn-secondary" onClick={closeObjectDialog}>
            {tr("common.cancel")}
          </button>
          <button class="btn-primary" disabled={!sql()} onClick={() => void ok()}>
            {tr("database.objects.scriptDialogs.rename")}
          </button>
        </>
      }
    >
      <div class="db-form dbo-pad">
        <span class="db-form-label">{tr("tools.database.renameToLabel", { value: p.column ? tr("tools.database.column", { table: p.table, from }) : `table ${from}` })}</span>
        <input
          class="dsd-input"
          value={name()}
          spellcheck={false}
          data-autofocus
          onInput={(e) => setName(e.currentTarget.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") void ok();
          }}
        />
      </div>
      <SqlPreview sql={sql()} dialect={dialect} placeholder={tr("database.objects.scriptDialogs.typeTheNewName")} />
    </ObjectModal>
  );
}

/** The options of the last Generate DDL (this run only). */
let lastOptions: DdlOptions = { includeDrops: false, ifNotExists: false, qualify: true };

const KIND_ORDER = ["schema", "type", "sequence", "table", "view", "matview", "routine", "index", "trigger"];

/** Generate DDL of `objects` without the dialog: to the clipboard, a console or a file. */
export async function generateDdlTo(target: "clipboard" | "console" | "file", ds: string, objects: DdlObject[]): Promise<void> {
  try {
    if (target === "file") {
      const name = objects.length === 1 ? `${objects[0].name || objects[0].schema}.sql` : "ddl.sql";
      if (isTauri()) {
        const { save } = await import("@tauri-apps/plugin-dialog");
        const path = await save({ defaultPath: name, filters: [{ name: "SQL", extensions: ["sql"] }] });
        if (!path) return;
        await dbObjectsApi.generateDdl(ds, objects, lastOptions, path);
        showBalloon(tr("database.objects.scriptDialogs.ddlSaved"), path);
        return;
      }
      const sql = await dbObjectsApi.generateDdl(ds, objects, lastOptions);
      const url = URL.createObjectURL(new Blob([sql], { type: "text/plain" }));
      const a = document.createElement("a");
      a.href = url;
      a.download = name;
      a.click();
      URL.revokeObjectURL(url);
      showBalloon(tr("database.objects.scriptDialogs.ddlSaved"), name);
      return;
    }
    const sql = await dbObjectsApi.generateDdl(ds, objects, lastOptions);
    if (target === "console") scriptToConsole(ds, sql);
    else {
      await writeClipboardText(sql);
      showBalloon(tr("database.objects.scriptDialogs.ddlCopied"), tr("tools.database.objects", { count: objects.length }));
    }
  } catch (e) {
    showBalloon(tr("database.objects.scriptDialogs.generateDdlFailed"), text(e));
  }
}

export function GenerateDdlDialog(p: { ds: string; objects: DdlObject[] }) {
  const ds = sourceById(p.ds);
  const dialect = sourceDialect(ds);
  const key = (o: DdlObject) => `${o.schema}/${o.kind}/${o.table ?? ""}/${o.name}`;
  const schemas = [...new Set(p.objects.map((o) => o.schema))];
  /** Every object of the schemas involved, for the checklist. */
  const all = createMemo<DdlObject[]>(() => {
    const list: DdlObject[] = [];
    for (const s of schemas) {
      const info = states[p.ds]?.introspected[s];
      if (!info) continue;
      for (const t of info.types ?? []) list.push({ schema: s, kind: "type", name: t.name });
      for (const q of info.sequences ?? []) list.push({ schema: s, kind: "sequence", name: q.name });
      for (const t of info.tables) list.push({ schema: s, kind: t.kind === "collection" ? "table" : t.kind, name: t.name });
      for (const r of info.routines ?? []) list.push({ schema: s, kind: "routine", name: r.name });
    }
    for (const o of p.objects) if (!list.some((x) => key(x) === key(o))) list.push(o);
    return list.sort((a, b) => KIND_ORDER.indexOf(a.kind) - KIND_ORDER.indexOf(b.kind) || a.name.localeCompare(b.name));
  });
  const initial = new Set(p.objects.some((o) => o.kind === "schema") ? all().map(key) : p.objects.map(key));
  const [picked, setPicked] = createSignal<Set<string>>(initial);
  const [opts, setOpts] = createSignal<DdlOptions>({ ...lastOptions });
  const [sql, setSql] = createSignal("");
  const selected = () => all().filter((o) => picked().has(key(o)) && o.kind !== "schema");
  createEffect(
    on([picked, opts], () => {
      lastOptions = opts();
      const objs = selected();
      if (!objs.length) return setSql("");
      dbObjectsApi
        .generateDdl(p.ds, objs, opts())
        .then(setSql)
        .catch((e) => setSql(`-- ${text(e)}`));
    }),
  );
  const toggle = (k: string, on: boolean) =>
    setPicked((s) => {
      const n = new Set(s);
      if (on) n.add(k);
      else n.delete(k);
      return n;
    });
  const option = (k: keyof DdlOptions, label: string) => (
    <label class="dbo-check">
      <input type="checkbox" checked={opts()[k]} onChange={(e) => setOpts({ ...opts(), [k]: e.currentTarget.checked })} data-testid={`dbo-ddl-${k}`} />
      {label}
    </label>
  );
  return (
    <ObjectModal
      title={tr("database.objects.scriptDialogs.sqlGeneratorGenerateDdl")}
      testId="dbo-ddl-dialog"
      width={1000}
      height={640}
      onClose={closeObjectDialog}
      footerStart={
        <div class="dbo-mode">
          {option("includeDrops", "Include DROP statements")}
          {option("ifNotExists", "IF NOT EXISTS / OR REPLACE")}
          {option("qualify", "Qualify names with schema")}
        </div>
      }
      footer={
        <>
          <button class="btn-secondary" disabled={!sql()} onClick={() => void generateDdlTo("file", p.ds, selected())} title={tr("database.objects.scriptDialogs.saveToFile")}>
            <FileDown size={14} /> {tr("database.objects.scriptDialogs.file")}
          </button>
          <button class="btn-secondary" disabled={!sql()} onClick={() => void writeClipboard(sql()).then((ok) => ok && showBalloon(tr("database.objects.scriptDialogs.ddlCopied"), `${selected().length} objects`))}>
            <Copy size={14} /> {tr("database.objects.scriptDialogs.copy")}
          </button>
          <button
            class="btn-primary"
            disabled={!sql()}
            onClick={() => {
              scriptToConsole(p.ds, sql());
              closeObjectDialog();
            }}
          >
            <SquareTerminal size={14} /> {tr("database.objects.scriptDialogs.openInConsole")}
          </button>
        </>
      }
    >
      <div class="dbo-ddl">
        <div class="dbo-ddl-objects" data-testid="dbo-ddl-objects">
          <For each={all()}>
            {(o) => (
              <label class="dbo-check">
                <input type="checkbox" checked={picked().has(key(o))} onChange={(e) => toggle(key(o), e.currentTarget.checked)} />
                <span class="dbo-kind">{o.kind === "matview" ? tr("tools.database.matView") : o.kind}</span>
                {schemas.length > 1 ? `${o.schema}.` : ""}
                {o.table ? `${o.table}.` : ""}
                {o.name}
              </label>
            )}
          </For>
        </div>
        <SqlPreview sql={sql()} dialect={dialect} placeholder={tr("database.objects.scriptDialogs.selectObjects")} testId="dbo-ddl-sql" />
      </div>
      <Show when={!all().length}>
        <div class="dsd-hint muted dbo-pad">{tr("database.objects.scriptDialogs.expandTheSchemaInThe")}</div>
      </Show>
    </ObjectModal>
  );
}
