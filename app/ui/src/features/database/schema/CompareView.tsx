/**
 * Compare Structure tab (DataGrip: Ctrl+D on two schemas): pick a source (how the schema
 * should be) and a target (the schema to migrate) — a schema of any data source, including
 * DDL data sources, or a pasted DDL snapshot — and see the diff tree of tables, views,
 * columns, keys, indexes, foreign keys and checks next to the migration script for the
 * target's dialect, both computed by `forge-dbschema`.
 *
 * The script is never run on its own: "Open in Console" puts it into a new console of the
 * target, and "Run Migration…" asks first (naming what it drops and whether the target is a
 * production data source) and is refused on read-only targets by the console guard. With
 * the Laravel or Doctrine plugin active, the migration can be written as a migration file.
 */
import { createMemo, createSignal, For, onMount, Show } from "solid-js";
import { ArrowLeftRight, ChevronDown, ChevronRight, Copy, FileCode, GitCompareArrows, Lock, Minus, Pencil, Play, Plus, SquareTerminal, TriangleAlert } from "lucide-solid";
import type { EditorTab } from "@/state/editor";
import { openFile } from "@/state/editor";
import { workspace } from "@/state/workspace";
import { workspaceApi } from "@/ipc/workspace";
import { isPluginActive } from "@/state/project";
import { askConfirm } from "@/features/contextMenu/dialogs";
import { balloonsFor } from "@/keymap";
import { dbApi, DRIVER_LABELS, modelOf, sourceDialect } from "@/ipc/db";
import { dbSchemaApi, type CompareResult, type CompareSide, type FrameworkKind, type ItemDiff, type TableDiff } from "@/ipc/dbSchema";
import { connect, defaultSchema, introspect, openConsole, sourceById, sources, visibleSchemas } from "../store";
import { loadSchema, nowSeconds, takeCompareStart } from "./model";
import { t as tr } from "@/i18n";
import { writeClipboard } from "@/features/clipboard/system";

const showBalloon = balloonsFor("Database");
const errorText = (e: unknown) => (e instanceof Error ? e.message : String(e));
const DDL = "__ddl__";

interface Side {
  id: string;
  schema: string;
  ddl: string;
}

const CATEGORY: Record<ItemDiff["category"], string> = {
  column: "column",
  primaryKey: "primary key",
  index: "index",
  foreignKey: "foreign key",
  check: "check",
  definition: "query",
  comment: "comment",
};

function KindIcon(props: { kind: TableDiff["kind"] }) {
  return (
    <span class={`cmp-kind ${props.kind}`}>
      <Show when={props.kind === "added"} fallback={<Show when={props.kind === "removed"} fallback={<Pencil size={11} />}><Minus size={12} /></Show>}>
        <Plus size={12} />
      </Show>
    </span>
  );
}

function SidePicker(props: { label: string; side: Side; onChange: (s: Side) => void; testid: string }) {
  const relational = () => sources().filter((s) => modelOf(s.driver) === "relational");
  const schemas = () => (props.side.id && props.side.id !== DDL ? visibleSchemas(props.side.id) : []);
  const pick = async (id: string) => {
    props.onChange({ ...props.side, id, schema: "" });
    if (id !== DDL && (await connect(id))) props.onChange({ id, schema: defaultSchema(id), ddl: props.side.ddl });
  };
  return (
    <div class="cmp-side" data-testid={props.testid}>
      <span class="cmp-side-label">{props.label}</span>
      <select class="cmp-select" value={props.side.id} onChange={(e) => void pick(e.currentTarget.value)}>
        <option value="" disabled>
          {tr("database.schema.compareView.dataSource")}
        </option>
        <For each={relational()}>{(s) => <option value={s.id}>{s.name}</option>}</For>
        <option value={DDL}>{tr("tools.database.ddlSnapshotPasteSql")}</option>
      </select>
      <Show when={props.side.id && props.side.id !== DDL}>
        <select class="cmp-select" value={props.side.schema} onChange={(e) => props.onChange({ ...props.side, schema: e.currentTarget.value })}>
          <For each={schemas()}>{(s) => <option value={s.name}>{s.name}</option>}</For>
        </select>
      </Show>
    </div>
  );
}

export function CompareView(props: { tab: EditorTab }) {
  const start = takeCompareStart(props.tab.path);
  const [source, setSource] = createSignal<Side>({ id: start.source?.id ?? "", schema: start.source?.schema ?? "", ddl: "" });
  const [target, setTarget] = createSignal<Side>({ id: start.target?.id ?? "", schema: start.target?.schema ?? "", ddl: "" });
  const [drops, setDrops] = createSignal(true);
  const [ignoreComments, setIgnoreComments] = createSignal(false);
  const [ignoreDefaults, setIgnoreDefaults] = createSignal(false);
  const [result, setResult] = createSignal<CompareResult | null>(null);
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [open, setOpen] = createSignal<Set<string>>(new Set());
  const [selected, setSelected] = createSignal<string | null>(null);

  const targetDs = () => (target().id && target().id !== DDL ? sourceById(target().id) : undefined);
  const dialect = () => {
    const t = target().id !== DDL ? sourceById(target().id) : undefined;
    const s = source().id !== DDL ? sourceById(source().id) : undefined;
    return sourceDialect(t ?? s);
  };
  const ready = () => {
    const ok = (s: Side) => (s.id === DDL ? !!s.ddl.trim() : !!s.id && !!s.schema);
    return ok(source()) && ok(target());
  };

  async function side(s: Side): Promise<CompareSide> {
    if (s.id === DDL) return { ddl: s.ddl };
    return { schema: await loadSchema(s.id, s.schema) };
  }

  async function compare(framework?: FrameworkKind): Promise<CompareResult | null> {
    if (!ready()) return null;
    setBusy(true);
    setError(null);
    try {
      const r = await dbSchemaApi.compare({
        dialect: dialect(),
        source: await side(source()),
        target: await side(target()),
        options: { drops: drops(), compare: { ignoreComments: ignoreComments(), ignoreDefaults: ignoreDefaults() } },
        framework,
        now: nowSeconds(),
      });
      setResult(r);
      setOpen(new Set(r.diff.tables.filter((t) => t.kind === "changed").map((t) => t.name)));
      return r;
    } catch (e) {
      setError(errorText(e));
      return null;
    } finally {
      setBusy(false);
    }
  }

  onMount(async () => {
    for (const [get, set] of [
      [source, setSource],
      [target, setTarget],
    ] as const) {
      const s = get();
      if (s.id && s.id !== DDL && (await connect(s.id)) && !s.schema) set({ ...s, schema: defaultSchema(s.id) });
    }
    if (ready()) void compare();
  });

  const swap = () => {
    const s = source();
    setSource(target());
    setTarget(s);
    if (ready()) void compare();
  };

  const toggle = (name: string) =>
    setOpen((o) => {
      const n = new Set(o);
      if (n.has(name)) n.delete(name);
      else n.add(name);
      return n;
    });

  const statements = () => result()?.migration.statements.length ?? 0;
  const readOnly = () => !!targetDs()?.options.readOnly;
  const canRun = () => !!targetDs() && targetDs()!.driver !== "ddl" && statements() > 0 && !readOnly() && !busy();

  function openInConsole(): void {
    const r = result();
    const t = targetDs();
    if (!r || !t) return;
    openConsole(t.id, r.migration.sql);
  }

  async function runMigration(): Promise<void> {
    const r = result();
    const t = targetDs();
    if (!r || !t || !canRun()) return;
    const sql = r.migration.sql;
    const g = await dbApi.guard(t.id, sql).catch(() => ({}) as { blocked?: string; confirm?: string });
    if (g.blocked) {
      showBalloon(tr("database.schema.compareView.readOnlyDataSource"), g.blocked);
      return;
    }
    const drops = r.migration.statements.filter((s) => s.destructive).length;
    const parts = [`Run ${statements()} statement${statements() === 1 ? "" : "s"} on ${t.name} (${target().schema})?`];
    if (drops) parts.push(`${drops} of them drop${drops === 1 ? "s" : ""} tables, columns or data.`);
    if (t.production) parts.push(`${t.name} is a production data source.`);
    if (g.confirm) parts.push(g.confirm);
    const ok = await askConfirm({ title: tr("database.schema.compareView.runMigration"), message: parts.join(" "), confirm: tr("database.schema.compareView.runMigration") });
    if (!ok) return;
    setBusy(true);
    let sessionId: string | null = null;
    try {
      const s = await dbApi.openSession(t.id);
      sessionId = s.id;
      const res = await dbApi.execute(s.id, sql, 100, true);
      const failed = res.find((x) => x.error);
      if (failed) showBalloon(tr("database.schema.compareView.migrationFailed"), `${failed.sql}\n${failed.error}`, 10000);
      else showBalloon(tr("database.schema.compareView.migrationComplete"), tr("tools.database.statementsOn", { count: res.length, name: t.name }));
      await introspect(t.id, target().schema);
    } catch (e) {
      showBalloon(tr("database.schema.compareView.migrationFailed"), errorText(e), 10000);
    } finally {
      if (sessionId) await dbApi.closeSession(sessionId).catch(() => undefined);
      setBusy(false);
    }
    await compare();
  }

  async function writeFramework(kind: FrameworkKind): Promise<void> {
    const r = await compare(kind);
    const root = workspace()?.root;
    if (!r?.file || !root) return;
    const path = `${root}/${r.file.path}`;
    try {
      await workspaceApi.createDir(path.slice(0, path.lastIndexOf("/"))).catch(() => undefined);
      await workspaceApi.writeFile(path, r.file.text);
      await openFile(path);
      showBalloon(tr("tools.database.migrationCreated", { value: kind === "laravel" ? "Laravel" : "Doctrine" }), r.file.path);
    } catch (e) {
      showBalloon(tr("database.schema.compareView.cannotWriteTheMigration"), errorText(e));
    }
  }

  const frameworks = createMemo(() => {
    const out: { kind: FrameworkKind; label: string }[] = [];
    if (isPluginActive("forge.laravel")) out.push({ kind: "laravel", label: tr("database.schema.compareView.laravelMigration") });
    if (isPluginActive("forge.doctrine")) out.push({ kind: "doctrine", label: tr("database.schema.compareView.doctrineMigration") });
    return out;
  });

  const sideName = (s: Side) => (s.id === DDL ? "DDL snapshot" : `${sourceById(s.id)?.name ?? s.id}${s.schema ? ` › ${s.schema}` : ""}`);

  return (
    <div class="cmp" data-testid="db-structure-compare">
      <div class="db-toolbar cmp-bar">
        <SidePicker label={tr("database.schema.compareView.source")} side={source()} onChange={setSource} testid="cmp-source" />
        <button class="icon-btn db-tool" title={tr("database.schema.compareView.swapSourceAndTarget")} onClick={swap}>
          <ArrowLeftRight size={14} />
        </button>
        <SidePicker label={tr("database.schema.compareView.target")} side={target()} onChange={setTarget} testid="cmp-target" />
        <span class="db-sep" />
        <label class="cmp-check" title={tr("database.schema.compareView.dropTablesViewsAndColumns")}>
          <input type="checkbox" checked={drops()} onChange={(e) => setDrops(e.currentTarget.checked)} /> {tr("database.schema.compareView.dropMissingObjects")}
        </label>
        <label class="cmp-check">
          <input type="checkbox" checked={ignoreComments()} onChange={(e) => setIgnoreComments(e.currentTarget.checked)} /> {tr("database.schema.compareView.ignoreComments")}
        </label>
        <label class="cmp-check">
          <input type="checkbox" checked={ignoreDefaults()} onChange={(e) => setIgnoreDefaults(e.currentTarget.checked)} /> {tr("database.schema.compareView.ignoreDefaults")}
        </label>
        <span class="db-spacer" />
        <button class="btn-primary cmp-btn" data-testid="cmp-run-compare" disabled={!ready() || busy()} onClick={() => void compare()}>
          <GitCompareArrows size={13} /> {tr("database.schema.compareView.compare")}
        </button>
      </div>
      <Show when={source().id === DDL || target().id === DDL}>
        <div class="cmp-ddl">
          <Show when={source().id === DDL}>
            <textarea class="cmp-ddl-text" placeholder={tr("database.schema.compareView.sourceDdlCreateTable")} value={source().ddl} onInput={(e) => setSource({ ...source(), ddl: e.currentTarget.value })} />
          </Show>
          <Show when={target().id === DDL}>
            <textarea class="cmp-ddl-text" placeholder={tr("database.schema.compareView.targetDdlCreateTable")} value={target().ddl} onInput={(e) => setTarget({ ...target(), ddl: e.currentTarget.value })} />
          </Show>
        </div>
      </Show>
      <div class="cmp-body">
        <div class="cmp-tree" data-testid="cmp-tree">
          <div class="cmp-pane-head">
            <span>{tr("database.schema.compareView.differences")}</span>
            <Show when={result()}>
              {(r) => (
                <span class="cmp-counts">
                  <span class="cmp-count added">+{r().diff.added}</span>
                  <span class="cmp-count changed">~{r().diff.changed}</span>
                  <span class="cmp-count removed">−{r().diff.removed}</span>
                </span>
              )}
            </Show>
          </div>
          <Show when={error()}>{(e) => <div class="cmp-empty error">{e()}</div>}</Show>
          <Show when={!result() && !error()}>
            <div class="cmp-empty">{busy() ? tr("database.schema.compareView.comparing") : tr("database.schema.compareView.chooseASourceAndA")}</div>
          </Show>
          <Show when={result()?.diff.tables.length === 0}>
            <div class="cmp-empty">{tr("database.schema.compareView.theStructuresAreIdentical")}</div>
          </Show>
          <For each={result()?.diff.tables ?? []}>
            {(t) => (
              <>
                <div
                  class="cmp-row table"
                  classList={{ selected: selected() === t.name }}
                  onClick={() => {
                    setSelected(t.name);
                    if (t.items?.length) toggle(t.name);
                  }}
                >
                  <span class="cmp-chevron">
                    <Show when={t.items?.length}>{open().has(t.name) ? <ChevronDown size={12} /> : <ChevronRight size={12} />}</Show>
                  </span>
                  <KindIcon kind={t.kind} />
                  <span class={`cmp-name ${t.kind}`}>{t.name}</span>
                  <span class="cmp-hint">
                    {t.tableKind === "table" ? "table" : t.tableKind === "matview" ? tr("tools.database.materializedView") : t.tableKind}
                    {t.kind === "added" ? tr("tools.database.onlyInSource") : t.kind === "removed" ? tr("tools.database.onlyInTarget") : tr("tools.database.differences", { count: t.items?.length, value: t.items?.length ?? 0 })}
                  </span>
                </div>
                <Show when={open().has(t.name)}>
                  <For each={t.items ?? []}>
                    {(i) => (
                      <div class="cmp-row item" title={[i.source && `source: ${i.source}`, i.target && `target: ${i.target}`].filter(Boolean).join("\n")}>
                        <span class="cmp-chevron" />
                        <KindIcon kind={i.kind} />
                        <span class="cmp-cat">{CATEGORY[i.category]}</span>
                        <span class={`cmp-name ${i.kind}`}>{i.name || tr("tools.database.unnamed2")}</span>
                        <span class="cmp-hint">
                          {i.changes?.length ? i.changes.join(", ") : i.kind === "added" ? i.source : i.kind === "removed" ? i.target : `${i.target ?? ""} → ${i.source ?? ""}`}
                        </span>
                      </div>
                    )}
                  </For>
                </Show>
              </>
            )}
          </For>
        </div>
        <div class="cmp-script">
          <div class="cmp-pane-head">
            <span>
              {tr("tools.database.migrationOf")} <b>{sideName(target())}</b>
            </span>
            <span class="muted">{result() ? tr("tools.database.statements", { count: statements(), value: DRIVER_LABELS[targetDs()?.driver ?? "postgres"] ?? dialect() }) : ""}</span>
          </div>
          <Show when={result()?.migration.warnings.length}>
            <div class="cmp-warnings">
              <For each={result()!.migration.warnings}>
                {(w) => (
                  <div class="cmp-warning">
                    <TriangleAlert size={12} /> {w}
                  </div>
                )}
              </For>
            </div>
          </Show>
          <pre class="cmp-sql" data-testid="cmp-sql">{result()?.migration.sql ?? ""}</pre>
          <div class="cmp-actions">
            <button class="btn-secondary cmp-btn" disabled={!result() || !targetDs()} onClick={openInConsole} title={tr("database.schema.compareView.openTheScriptInA")}>
              <SquareTerminal size={13} /> {tr("database.schema.compareView.openInConsole")}
            </button>
            <button class="btn-secondary cmp-btn" disabled={!result()} onClick={() => void writeClipboard(result()?.migration.sql ?? "").then((ok) => ok && showBalloon(tr("database.schema.compareView.migrationScriptCopied")))}>
              <Copy size={13} /> {tr("database.schema.compareView.copy")}
            </button>
            <For each={frameworks()}>
              {(f) => (
                <button class="btn-secondary cmp-btn" disabled={!result() || busy()} onClick={() => void writeFramework(f.kind)} title={tr("database.schema.compareView.writeTheMigrationAsA", { label: f.label.toLowerCase() })}>
                  <FileCode size={13} /> {f.label}
                </button>
              )}
            </For>
            <span class="db-spacer" />
            <Show when={readOnly()}>
              <span class="db-badge" title={tr("database.schema.compareView.theTargetIsReadOnly")}>
                <Lock size={11} /> read-only
              </span>
            </Show>
            <button class="btn-secondary cmp-btn cmp-danger" data-testid="cmp-run" disabled={!canRun()} onClick={() => void runMigration()} title={tr("database.schema.compareView.runTheScriptOnThe")}>
              <Play size={13} /> {tr("database.schema.compareView.runMigration2")}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
