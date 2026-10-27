/**
 * Dump and restore with the databases' own tools (DataGrip's "Export with 'mysqldump'" and
 * "Restore with 'pg_restore'"): the tool's path, the file, the objects to include and the
 * options, with the command line built in Rust. Runs in the Run tool window with the data
 * source's connection (password in the environment, through its SSH tunnel). Restores ask
 * first, are refused on read-only data sources and warned about on production ones.
 */
import { createEffect, createResource, createSignal, For, on, Show } from "solid-js";
import { createStore } from "solid-js/store";
import { FolderOpen } from "lucide-solid";
import { isTauri } from "@/ipc";
import { dbObjectsApi, defaultDumpRequest, type DumpAction, type DumpRequest, type DumpTool } from "@/ipc/dbObjects";
import { askConfirm } from "@/features/contextMenu/dialogs";
import { sourceById, states, visibleSchemas } from "../store";
import { ObjectModal } from "./Modal";
import { closeObjectDialog } from "./state";
import { startDump } from "./dumpRun";

const PROGRAM: Record<DumpTool, string> = {
  mysqldump: "mysqldump",
  mysql: "mysql",
  pgDump: "pg_dump",
  pgRestore: "pg_restore",
  psql: "psql",
  sqlite3: "sqlite3",
  sqlcmd: "sqlcmd",
};

const EXTENSION: Partial<Record<DumpTool, string>> = { pgDump: "dump", sqlcmd: "bak" };

export function DumpDialog(p: { ds: string; tool: DumpTool; action: DumpAction; schema?: string; tables: string[] }) {
  const ds = sourceById(p.ds);
  const [tools] = createResource(() => dbObjectsApi.dumpTools(p.ds).catch(() => []));
  const [req, setReq] = createStore<DumpRequest>({
    ...defaultDumpRequest(p.tool, p.action),
    format: p.tool === "pgDump" ? "custom" : "plain",
    schemas: p.schema && p.tool !== "sqlite3" ? [p.schema] : [],
    tables: p.tables,
  });
  const [command, setCommand] = createSignal("");
  const [problem, setProblem] = createSignal("");
  const restore = () => req.action === "restore";
  const program = () => PROGRAM[req.tool];
  const pg = () => req.tool === "pgDump" || req.tool === "pgRestore";

  createEffect(
    on(
      () => JSON.stringify(req),
      () => {
        dbObjectsApi
          .dumpCommand(p.ds, { ...req, schemas: [...req.schemas], tables: [...req.tables] })
          .then((c) => {
            setCommand(c);
            setProblem("");
          })
          .catch((e) => {
            setCommand("");
            setProblem(e instanceof Error ? e.message : String(e));
          });
      },
    ),
  );

  const browse = async () => {
    const ext = EXTENSION[req.tool] ?? "sql";
    if (!isTauri()) {
      setReq("file", req.file || `${ds?.database || "dump"}.${ext}`);
      return;
    }
    const dialog = await import("@tauri-apps/plugin-dialog");
    const path = restore()
      ? await dialog.open({ multiple: false, filters: [{ name: "Dump files", extensions: ["sql", "dump", "backup", "bak", "tar", "gz"] }] })
      : await dialog.save({ defaultPath: `${ds?.database || "dump"}.${ext}` });
    if (typeof path === "string") setReq("file", path);
  };

  const tables = () => {
    const out: string[] = [];
    for (const s of req.schemas.length ? req.schemas : [p.schema ?? ""]) {
      for (const t of states[p.ds]?.introspected[s]?.tables ?? []) if (t.kind === "table") out.push(req.tool === "pgDump" ? `${s}.${t.name}` : t.name);
    }
    return out;
  };

  const toggle = (k: "schemas" | "tables", v: string, on: boolean) => setReq(k, (list) => (on ? [...list.filter((x) => x !== v), v] : list.filter((x) => x !== v)));

  const run = async () => {
    if (restore()) {
      const ok = await askConfirm({
        title: `Restore with '${program()}'`,
        message: `Restore ${req.file} into ${ds?.name ?? "the data source"}? Existing objects and data may be overwritten.`,
        confirm: "Restore",
      });
      if (!ok) return;
    }
    const started = await startDump(p.ds, { ...req, schemas: [...req.schemas], tables: [...req.tables] }, `${restore() ? "Restore" : "Dump"} ${ds?.name ?? ""} (${program()})`);
    if (started) closeObjectDialog();
  };

  return (
    <ObjectModal
      title={`${restore() ? "Restore with" : "Export with"} '${program()}'`}
      testId="dbo-dump-dialog"
      width={760}
      onClose={closeObjectDialog}
      footer={
        <>
          <button class="btn-secondary" onClick={closeObjectDialog}>
            Cancel
          </button>
          <button class="btn-primary" disabled={!command() || (restore() && ds?.options.readOnly)} onClick={() => void run()} data-testid="dbo-dump-run">
            Run
          </button>
        </>
      }
    >
      <div class="db-form dbo-pad">
        <span class="db-form-label">Tool:</span>
        <select
          class="dsd-input"
          value={`${req.tool}:${req.action}`}
          onChange={(e) => {
            const [tool, action] = e.currentTarget.value.split(":") as [DumpTool, DumpAction];
            setReq({ tool, action, format: tool === "pgDump" ? "custom" : "plain" });
          }}
        >
          <For each={tools() ?? []}>{(t) => <option value={`${t.tool}:${t.action}`}>{t.label}</option>}</For>
        </select>
        <span class="db-form-label">Path to {program()}:</span>
        <input class="dsd-input mono" value={req.program} placeholder={`${program()} (from PATH)`} spellcheck={false} onInput={(e) => setReq("program", e.currentTarget.value)} data-autofocus />
        <span class="db-form-label">{restore() ? "Input file:" : "Output file:"}</span>
        <div class="dbo-row">
          <input class="dsd-input mono" value={req.file} placeholder={restore() ? "dump to restore" : "where to write the dump"} spellcheck={false} onInput={(e) => setReq("file", e.currentTarget.value)} data-testid="dbo-dump-file" />
          <button class="btn-secondary" onClick={() => void browse()} title="Browse">
            <FolderOpen size={14} />
          </button>
        </div>
        <Show when={!restore() && req.tool !== "sqlite3" && req.tool !== "sqlcmd"}>
          <span class="db-form-label">{req.tool === "mysqldump" ? "Databases:" : "Schemas:"}</span>
          <div class="dbo-picker">
            <For each={visibleSchemas(p.ds)}>
              {(s) => (
                <label class="dbo-check">
                  <input type="checkbox" checked={req.schemas.includes(s.name)} onChange={(e) => toggle("schemas", s.name, e.currentTarget.checked)} />
                  {s.name}
                </label>
              )}
            </For>
          </div>
        </Show>
        <Show when={!restore() && req.tool !== "sqlcmd"}>
          <span class="db-form-label">Tables:</span>
          <div class="dbo-picker dbo-picker-tall">
            <For each={tables()} fallback={<span class="muted">All tables (expand a schema to choose)</span>}>
              {(t) => (
                <label class="dbo-check">
                  <input type="checkbox" checked={req.tables.includes(t)} onChange={(e) => toggle("tables", t, e.currentTarget.checked)} />
                  {t}
                </label>
              )}
            </For>
          </div>
        </Show>
        <Show when={req.tool === "pgDump"}>
          <span class="db-form-label">Format:</span>
          <select class="dsd-input" value={req.format} onChange={(e) => setReq("format", e.currentTarget.value)}>
            <option value="plain">Plain SQL</option>
            <option value="custom">Custom (pg_restore)</option>
            <option value="directory">Directory</option>
            <option value="tar">Tar</option>
          </select>
        </Show>
        <Show when={pg() && (req.format === "directory" || req.tool === "pgRestore")}>
          <span class="db-form-label">Jobs:</span>
          <input class="dsd-input dbo-num" type="number" min="0" value={req.jobs} onInput={(e) => setReq("jobs", Math.max(0, Number(e.currentTarget.value) || 0))} />
        </Show>
        <span class="db-form-label">Options:</span>
        <div class="dbo-checks">
          <Show when={req.tool !== "mysql" && req.tool !== "sqlcmd" && req.tool !== "psql"}>
            <label class="dbo-check">
              <input type="checkbox" checked={req.structure} onChange={(e) => setReq("structure", e.currentTarget.checked)} />
              Structure
            </label>
            <label class="dbo-check">
              <input type="checkbox" checked={req.data} onChange={(e) => setReq("data", e.currentTarget.checked)} />
              Data
            </label>
          </Show>
          <Show when={req.tool !== "sqlite3" && req.tool !== "psql" && req.tool !== "mysql"}>
            <label class="dbo-check">
              <input type="checkbox" checked={req.dropObjects} onChange={(e) => setReq("dropObjects", e.currentTarget.checked)} />
              {req.tool === "sqlcmd" ? (restore() ? "Replace the database" : "Overwrite the backup") : "Drop objects first"}
            </label>
          </Show>
          <Show when={req.tool === "mysqldump" || pg()}>
            <label class="dbo-check">
              <input type="checkbox" checked={req.createDatabase} onChange={(e) => setReq("createDatabase", e.currentTarget.checked)} />
              Create database
            </label>
          </Show>
          <Show when={req.tool === "mysqldump" || req.tool === "psql"}>
            <label class="dbo-check">
              <input type="checkbox" checked={req.singleTransaction} onChange={(e) => setReq("singleTransaction", e.currentTarget.checked)} />
              Single transaction
            </label>
          </Show>
          <Show when={pg()}>
            <label class="dbo-check">
              <input type="checkbox" checked={req.noOwner} onChange={(e) => setReq("noOwner", e.currentTarget.checked)} />
              No owner
            </label>
          </Show>
        </div>
        <span class="db-form-label">More arguments:</span>
        <input class="dsd-input mono" value={req.extra} placeholder="--verbose" spellcheck={false} onInput={(e) => setReq("extra", e.currentTarget.value)} />
      </div>
      <div class="dbo-preview-label">Command line</div>
      <pre class="dbo-sql dbo-command" data-testid="dbo-dump-command">
        {command() || problem()}
      </pre>
      <div class="dsd-hint muted dbo-pad">
        The password is passed in the environment ({req.tool.startsWith("my") ? "MYSQL_PWD" : pg() || req.tool === "psql" ? "PGPASSWORD" : req.tool === "sqlcmd" ? "SQLCMDPASSWORD" : "none"}), never on the command line.
        {ds?.options.ssh?.enabled ? " The tool connects through the data source's SSH tunnel." : ""}
        <Show when={restore() && ds?.options.readOnly}>
          {" "}
          <span class="dbo-danger-text">The data source is read-only: restoring is not allowed.</span>
        </Show>
      </div>
    </ObjectModal>
  );
}

