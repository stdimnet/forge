/**
 * Import Data from File (DataGrip): a CSV/TSV, Excel or JSON file into an existing or a new
 * table. The source side (format, delimiter, quote, encoding, header row, sheet) is read and
 * typed in Rust (`forge-dbtransfer`); the mapping assigns source columns to target columns
 * (a new table gets a generated CREATE TABLE); rows go in as batched INSERTs with progress,
 * skip-or-abort error handling and Cancel.
 */
import { batch, createEffect, createMemo, createSignal, For, on, onCleanup, Show } from "solid-js";
import { createStore } from "solid-js/store";
import { FileUp, TriangleAlert } from "lucide-solid";
import { isTauri } from "@/ipc";
import { sourceDialect } from "@/ipc/db";
import {
  dbObjectsApi,
  defaultReadOptions,
  emptyTable,
  formatOf,
  type ColumnGuess,
  type ImportPreview,
  type ImportPlan,
  type ImportSource,
  type ImportSummary,
  type ReadOptions,
  type TableSpec,
  type ValueKind,
} from "@/ipc/dbObjects";
import { askConfirm } from "@/features/contextMenu/dialogs";
import { showBalloon } from "@/keymap";
import { sourceById, visibleSchemas } from "../store";
import { ObjectModal } from "./Modal";
import { SqlPreview } from "./SqlPreview";
import { afterChange, closeObjectDialog, loadedTables } from "./state";
import { t as tr } from "@/i18n";

const text = (e: unknown) => (e instanceof Error ? e.message : String(e));

const KINDS: ValueKind[] = ["integer", "decimal", "boolean", "date", "datetime", "text"];
const DELIMITERS: [string, string][] = [
  ["", "Detect"],
  [",", "Comma ,"],
  [";", "Semicolon ;"],
  ["\t", "Tab"],
  ["|", "Pipe |"],
];
const ENCODINGS = ["", "UTF-8", "UTF-16LE", "UTF-16BE", "windows-1252", "ISO-8859-1", "ISO-8859-15", "Shift_JIS", "GBK"];

interface MapRow {
  include: boolean;
  target: string;
  kind: ValueKind;
  /** New table: the column's SQL type. */
  type: string;
}

/** A file name as a table name (the import's default). */
const tableName = (file: string) =>
  file
    .replace(/\.[^.]+$/, "")
    .toLowerCase()
    .replace(/[^\p{L}\p{N}]+/gu, "_")
    .replace(/^_+|_+$/g, "") || "imported";

export function ImportDialog(p: { ds: string; schema: string; table?: string }) {
  const ds = sourceById(p.ds);
  const dialect = sourceDialect(ds);
  let fileInput!: HTMLInputElement;
  const [file, setFile] = createSignal<{ name: string; source: ImportSource } | null>(null);
  const [opts, setOpts] = createStore<ReadOptions>(defaultReadOptions());
  const [preview, setPreview] = createSignal<ImportPreview | null>(null);
  const [error, setError] = createSignal("");
  const [rowsShown, setRowsShown] = createSignal(20);
  const [schema, setSchema] = createSignal(p.schema);
  const [target, setTarget] = createSignal<"existing" | "new">(p.table ? "existing" : "new");
  const [table, setTable] = createSignal(p.table ?? "");
  const [newName, setNewName] = createSignal("");
  const [mapping, setMapping] = createStore<MapRow[]>([]);
  const [onError, setOnError] = createSignal<"skip" | "abort">("skip");
  const [batchSize, setBatchSize] = createSignal(500);
  const [emptyAsNull, setEmptyAsNull] = createSignal(true);
  const [createSql, setCreateSql] = createSignal("");
  const [running, setRunning] = createSignal<{ id: string; done: number; total: number } | null>(null);
  const [result, setResult] = createSignal<ImportSummary | null>(null);

  const existing = createMemo(() => loadedTables(p.ds).filter((t) => t.schema === schema()));
  const targetColumns = () => existing().find((t) => t.name === table())?.columns ?? [];

  const pick = async () => {
    if (isTauri()) {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const path = await open({ multiple: false, filters: [{ name: "Data files", extensions: ["csv", "tsv", "txt", "xlsx", "xls", "ods", "json", "ndjson", "jsonl"] }] });
      if (typeof path === "string") choose(path.split(/[\\/]/).pop() ?? path, { path });
      return;
    }
    fileInput.click();
  };

  const choose = (name: string, source: ImportSource) => {
    batch(() => {
      setFile({ name, source });
      setOpts({ ...defaultReadOptions(formatOf(name)) });
      if (!p.table) setNewName(tableName(name));
    });
  };

  const onFile = async (f: File | undefined) => {
    if (!f) return;
    const bytes = new Uint8Array(await f.arrayBuffer());
    let bin = "";
    for (let i = 0; i < bytes.length; i += 0x8000) bin += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
    choose(f.name, { data: btoa(bin) });
  };

  // Read the source again when the file or a source option changes.
  createEffect(
    on([file, () => JSON.stringify(opts), rowsShown], ([f]) => {
      if (!f) return;
      dbObjectsApi
        .importPreview(f.source, { ...opts }, rowsShown(), dialect)
        .then((pv) => {
          setError("");
          const same = mapping.length === pv.guesses.length;
          setPreview(pv);
          setMapping(
            pv.guesses.map((g: ColumnGuess, i: number) => ({
              include: same ? mapping[i].include : true,
              target: same && mapping[i].target ? mapping[i].target : pv.names[i],
              kind: same ? mapping[i].kind : g.kind,
              type: same && mapping[i].type ? mapping[i].type : (pv.types[i] ?? ""),
            })),
          );
        })
        .catch((e) => {
          setPreview(null);
          setError(text(e));
        });
    }),
  );

  // Existing table: map source columns to its columns by name.
  createEffect(
    on([table, target, () => preview()?.table.columns.join("\u0000")], () => {
      if (target() !== "existing") return;
      const cols = targetColumns();
      if (!cols.length) return;
      mapping.forEach((m, i) => {
        const match = cols.find((c) => c.toLowerCase() === m.target.toLowerCase() || c.toLowerCase() === preview()?.names[i]);
        setMapping(i, { target: match ?? "", include: !!match });
      });
    }),
  );

  /** The new table of the mapping (types from Rust's detection, editable). */
  const newTable = (): TableSpec | null => {
    const pv = preview();
    if (target() !== "new" || !pv) return null;
    const t = emptyTable(schema(), newName().trim());
    t.columns = mapping
      .filter((m) => m.include && m.target.trim())
      .map((m) => ({ name: m.target.trim(), dataType: m.type, nullable: true, comment: "", autoIncrement: false }));
    return t;
  };

  createEffect(
    on([() => JSON.stringify(mapping), target, newName, schema], () => {
      const t = newTable();
      if (!t || !t.name || !t.columns.length) return setCreateSql("");
      dbObjectsApi
        .designPreview(dialect, t)
        .then((pv) => setCreateSql(pv.sql))
        .catch((e) => setCreateSql(`-- ${text(e)}`));
    }),
  );

  const plan = (): ImportPlan => ({
    schema: schema(),
    table: target() === "new" ? newName().trim() : table(),
    columns: mapping.map((m, i) => ({ source: i, target: m.target.trim(), kind: m.kind })).filter((c, i) => mapping[i].include && c.target),
    create: newTable(),
    batchSize: batchSize(),
    onError: onError(),
    emptyAsNull: emptyAsNull(),
    limit: 0,
  });

  let unlisten: (() => void) | undefined;
  onCleanup(() => unlisten?.());

  const run = async (confirmed = false) => {
    const f = file();
    if (!f) return;
    const importId = `import-${Date.now().toString(36)}`;
    setResult(null);
    setRunning({ id: importId, done: 0, total: preview()?.totalRows ?? 0 });
    unlisten?.();
    unlisten = await dbObjectsApi.onImport((e) => {
      if (e.importId === importId) setRunning({ id: importId, done: e.done, total: e.total });
    });
    try {
      const summary = await dbObjectsApi.importRun({ id: p.ds, importId, source: f.source, options: { ...opts }, plan: plan(), confirmed });
      setResult(summary);
      const where = `${schema()}.${plan().table}`;
      showBalloon(
        summary.cancelled ? tr("database.objects.importDialog.importCancelled") : summary.aborted ? tr("database.objects.importDialog.importAborted") : tr("database.objects.importDialog.importFinished"),
        `${summary.inserted} row${summary.inserted === 1 ? "" : "s"} into ${where}${summary.skipped ? `, ${summary.skipped} skipped` : ""}`,
      );
      await afterChange(p.ds, schema());
    } catch (e) {
      const msg = text(e);
      if (!confirmed && /production data source/.test(msg)) {
        setRunning(null);
        if (await askConfirm({ title: tr("database.objects.importDialog.productionDataSource"), message: msg, confirm: tr("database.objects.importDialog.import") })) return run(true);
        return;
      }
      showBalloon(tr("database.objects.importDialog.importFailed"), msg);
    } finally {
      setRunning(null);
      unlisten?.();
      unlisten = undefined;
    }
  };

  const ready = () =>
    !!preview() && !running() && plan().columns.length > 0 && (target() === "existing" ? !!table() : !!newName().trim()) && !ds?.options.readOnly;

  const kindLabel = (k: ValueKind) => ({ integer: "integer", decimal: "decimal", boolean: "boolean", date: "date", datetime: "date-time", text: "text" })[k];

  return (
    <ObjectModal
      title={`Import "${file()?.name ?? "file"}"`}
      testId="dbo-import-dialog"
      width={1060}
      height={760}
      onClose={() => {
        const r = running();
        if (r) void dbObjectsApi.importCancel(r.id);
        closeObjectDialog();
      }}
      footerStart={
        <Show
          when={running()}
          fallback={
            <Show when={result()}>
              {(r) => (
                <span class="muted" data-testid="dbo-import-result">
                  {r().inserted} imported{r().skipped ? `, ${r().skipped} skipped` : ""}
                  {r().errors[0] ? ` — row ${r().errors[0].row + 1}: ${r().errors[0].message}` : ""}
                </span>
              )}
            </Show>
          }
        >
          {(r) => (
            <div class="dbo-progress" data-testid="dbo-import-progress">
              <div class="dbo-progress-bar">
                <div style={{ width: `${r().total ? Math.round((r().done / r().total) * 100) : 0}%` }} />
              </div>
              <span>
                {r().done} / {r().total}
              </span>
              <button class="btn-secondary" onClick={() => void dbObjectsApi.importCancel(r().id)}>
                Cancel
              </button>
            </div>
          )}
        </Show>
      }
      footer={
        <>
          <button class="btn-secondary" onClick={closeObjectDialog}>
            Close
          </button>
          <button class="btn-primary" disabled={!ready()} onClick={() => void run()} data-testid="dbo-import-ok">
            Import
          </button>
        </>
      }
    >
      <input type="file" ref={fileInput} hidden accept=".csv,.tsv,.txt,.xlsx,.xls,.ods,.json,.ndjson,.jsonl" data-testid="dbo-import-file" onChange={(e) => void onFile(e.currentTarget.files?.[0])} />
      <div class="dbo-import">
        <section class="dbo-import-side">
          <div class="dbo-section-title">{tr("database.objects.importDialog.source")}</div>
          <div class="db-form">
            <span class="db-form-label">{tr("database.objects.importDialog.file")}</span>
            <div class="dbo-row">
              <span class="dbo-file" title={file()?.source.path ?? file()?.name}>
                {file()?.name ?? "No file chosen"}
              </span>
              <button class="btn-secondary" onClick={() => void pick()} data-autofocus>
                <FileUp size={14} /> {tr("database.objects.importDialog.choose")}
              </button>
            </div>
            <span class="db-form-label">{tr("database.objects.importDialog.format")}</span>
            <select class="dsd-input" value={opts.format} onChange={(e) => setOpts("format", e.currentTarget.value as ReadOptions["format"])}>
              <option value="csv">{tr("database.objects.importDialog.csv")}</option>
              <option value="tsv">{tr("database.objects.importDialog.tsv")}</option>
              <option value="excel">{tr("database.objects.importDialog.excel")}</option>
              <option value="json">{tr("database.objects.importDialog.jsonNdjson")}</option>
            </select>
            <Show when={opts.format === "csv" || opts.format === "tsv"}>
              <span class="db-form-label">{tr("database.objects.importDialog.delimiter")}</span>
              <select class="dsd-input" value={opts.delimiter ?? ""} onChange={(e) => setOpts("delimiter", e.currentTarget.value || null)}>
                <For each={DELIMITERS}>{([v, l]) => <option value={v}>{v === "" && preview()?.table.delimiter ? `${l} (${preview()!.table.delimiter === "\t" ? "tab" : preview()!.table.delimiter})` : l}</option>}</For>
              </select>
              <span class="db-form-label">{tr("database.objects.importDialog.quote")}</span>
              <select class="dsd-input" value={opts.quote} onChange={(e) => setOpts("quote", e.currentTarget.value)}>
                <option value={'"'}>Double quote "</option>
                <option value="'">{tr("database.objects.importDialog.singleQuote")}</option>
              </select>
            </Show>
            <Show when={opts.format !== "excel"}>
              <span class="db-form-label">{tr("database.objects.importDialog.encoding")}</span>
              <select class="dsd-input" value={opts.encoding} onChange={(e) => setOpts("encoding", e.currentTarget.value)}>
                <For each={ENCODINGS}>{(enc) => <option value={enc}>{enc || `Detect${preview()?.table.encoding ? ` (${preview()!.table.encoding})` : ""}`}</option>}</For>
              </select>
            </Show>
            <Show when={opts.format === "excel" && preview()?.table.sheets?.length}>
              <span class="db-form-label">{tr("database.objects.importDialog.sheet")}</span>
              <select class="dsd-input" value={opts.sheet || preview()!.table.sheets![0]} onChange={(e) => setOpts("sheet", e.currentTarget.value)}>
                <For each={preview()!.table.sheets!}>{(s) => <option>{s}</option>}</For>
              </select>
            </Show>
            <span />
            <div class="dbo-checks">
              <label class="dbo-check">
                <input type="checkbox" checked={opts.header} onChange={(e) => setOpts("header", e.currentTarget.checked)} />
                {tr("database.objects.importDialog.firstRowIsHeader")}
              </label>
              <label class="dbo-check">
                <input type="checkbox" checked={opts.trim} onChange={(e) => setOpts("trim", e.currentTarget.checked)} />
                {tr("database.objects.importDialog.trimWhitespace")}
              </label>
            </div>
            <span class="db-form-label">{tr("database.objects.importDialog.skipRows")}</span>
            <input class="dsd-input dbo-num" type="number" min="0" value={opts.skipRows} onInput={(e) => setOpts("skipRows", Math.max(0, Number(e.currentTarget.value) || 0))} />
          </div>
          <div class="dbo-section-title">{tr("database.objects.importDialog.target")}</div>
          <div class="db-form">
            <span class="db-form-label">{tr("database.objects.importDialog.schema")}</span>
            <select class="dsd-input" value={schema()} onChange={(e) => setSchema(e.currentTarget.value)}>
              <For each={visibleSchemas(p.ds)}>{(s) => <option value={s.name}>{s.name}</option>}</For>
            </select>
            <span class="db-form-label">{tr("database.objects.importDialog.table")}</span>
            <div class="dbo-row">
              <label class="dbo-check">
                <input type="radio" name="dbo-target" checked={target() === "existing"} onChange={() => setTarget("existing")} />
                Existing
              </label>
              <label class="dbo-check">
                <input type="radio" name="dbo-target" checked={target() === "new"} onChange={() => setTarget("new")} data-testid="dbo-import-new" />
                New
              </label>
            </div>
            <span />
            <Show
              when={target() === "existing"}
              fallback={<input class="dsd-input" value={newName()} placeholder="new_table" spellcheck={false} onInput={(e) => setNewName(e.currentTarget.value)} data-testid="dbo-import-table" />}
            >
              <select class="dsd-input" value={table()} onChange={(e) => setTable(e.currentTarget.value)}>
                <option value="">{tr("database.objects.importDialog.chooseATable")}</option>
                <For each={existing()}>{(t) => <option value={t.name}>{t.name}</option>}</For>
              </select>
            </Show>
          </div>
          <div class="dbo-section-title">{tr("database.objects.importDialog.options")}</div>
          <div class="db-form">
            <span class="db-form-label">{tr("database.objects.importDialog.onError")}</span>
            <select class="dsd-input" value={onError()} onChange={(e) => setOnError(e.currentTarget.value as "skip" | "abort")}>
              <option value="skip">{tr("database.objects.importDialog.skipTheRowAndContinue")}</option>
              <option value="abort">Abort (roll back)</option>
            </select>
            <span class="db-form-label">{tr("database.objects.importDialog.batchSize")}</span>
            <input class="dsd-input dbo-num" type="number" min="1" value={batchSize()} onInput={(e) => setBatchSize(Math.max(1, Number(e.currentTarget.value) || 1))} />
            <span />
            <label class="dbo-check">
              <input type="checkbox" checked={emptyAsNull()} onChange={(e) => setEmptyAsNull(e.currentTarget.checked)} />
              Empty values as NULL
            </label>
          </div>
        </section>
        <section class="dbo-import-main">
          <Show when={error()}>
            <div class="dbo-warnings">
              <div class="destructive">
                <TriangleAlert size={13} /> {error()}
              </div>
            </div>
          </Show>
          <Show when={preview()?.table.warnings?.length}>
            <div class="dbo-warnings">
              <For each={preview()!.table.warnings!}>
                {(w) => (
                  <div>
                    <TriangleAlert size={13} /> {w}
                  </div>
                )}
              </For>
            </div>
          </Show>
          <div class="dbo-section-title">{tr("database.objects.importDialog.columns")}</div>
          <div class="dbo-grid-wrap dbo-mapping">
            <table class="dbo-grid" data-testid="dbo-import-mapping">
              <thead>
                <tr>
                  <th />
                  <th>{tr("database.objects.importDialog.sourceColumn")}</th>
                  <th>{tr("database.objects.importDialog.detected")}</th>
                  <th>{tr("database.objects.importDialog.targetColumn")}</th>
                  <Show when={target() === "new"}>
                    <th>{tr("database.objects.importDialog.type")}</th>
                  </Show>
                </tr>
              </thead>
              <tbody>
                <For each={preview()?.table.columns ?? []}>
                  {(c, i) => (
                    <tr classList={{ muted: !mapping[i()]?.include }}>
                      <td>
                        <input type="checkbox" checked={mapping[i()]?.include} onChange={(e) => setMapping(i(), "include", e.currentTarget.checked)} />
                      </td>
                      <td>{c}</td>
                      <td>
                        <select class="dbo-cell-input" value={mapping[i()]?.kind} onChange={(e) => setMapping(i(), "kind", e.currentTarget.value as ValueKind)}>
                          <For each={KINDS}>{(k) => <option value={k}>{kindLabel(k)}</option>}</For>
                        </select>
                      </td>
                      <td>
                        <Show
                          when={target() === "existing"}
                          fallback={<input class="dbo-cell-input" value={mapping[i()]?.target ?? ""} spellcheck={false} onInput={(e) => setMapping(i(), "target", e.currentTarget.value)} />}
                        >
                          <select class="dbo-cell-input" value={mapping[i()]?.target ?? ""} onChange={(e) => setMapping(i(), { target: e.currentTarget.value, include: !!e.currentTarget.value })}>
                            <option value="">(skip)</option>
                            <For each={targetColumns()}>{(t) => <option value={t}>{t}</option>}</For>
                          </select>
                        </Show>
                      </td>
                      <Show when={target() === "new"}>
                        <td>
                          <input class="dbo-cell-input mono" value={mapping[i()]?.type ?? ""} spellcheck={false} onInput={(e) => setMapping(i(), "type", e.currentTarget.value)} />
                        </td>
                      </Show>
                    </tr>
                  )}
                </For>
              </tbody>
            </table>
          </div>
          <div class="dbo-section-title">
            Preview
            <span class="muted">
              {preview() ? ` first ${Math.min(rowsShown(), preview()!.totalRows)} of ${preview()!.totalRows} rows` : ""}
            </span>
            <select class="dbo-cell-input dbo-rows" value={rowsShown()} onChange={(e) => setRowsShown(Number(e.currentTarget.value))}>
              <For each={[10, 20, 50, 100]}>{(n) => <option value={n}>{n} rows</option>}</For>
            </select>
          </div>
          <div class="dbo-grid-wrap dbo-rows-grid">
            <table class="dbo-grid" data-testid="dbo-import-preview">
              <thead>
                <tr>
                  <For each={preview()?.table.columns ?? []}>{(c) => <th>{c}</th>}</For>
                </tr>
              </thead>
              <tbody>
                <For each={preview()?.table.rows ?? []}>
                  {(r) => (
                    <tr>
                      <For each={r}>{(v) => <td classList={{ "dbo-null": v === null }}>{v === null ? "<null>" : v}</td>}</For>
                    </tr>
                  )}
                </For>
              </tbody>
            </table>
          </div>
          <Show when={target() === "new" && createSql()}>
            <div class="dbo-section-title">{tr("database.objects.importDialog.newTable")}</div>
            <SqlPreview sql={createSql()} dialect={dialect} testId="dbo-import-create" />
          </Show>
        </section>
      </div>
    </ObjectModal>
  );
}
