/**
 * Entity Framework Core, like Rider's EF Core actions: the **EF Core** tool window lists
 * the DbContexts (with their DbSets) and the migrations of the sources, marked applied or
 * pending after a `dotnet ef migrations list`; Add Migration, Remove Last Migration,
 * Update Database and Scaffold DbContext open a dialog and run `dotnet ef` in the Run tool
 * window. Models, command lines and output are computed in Rust (`dotnet_ef_*`).
 */
import { createMemo, createResource, createSignal, For, onCleanup, Show, createRoot } from "solid-js";
import { render } from "solid-js/web";
import { Check, CircleDashed, Clock, Database, DatabaseZap, FileCode, Minus, Plus, RefreshCw, Table2, Wand2 } from "lucide-solid";
import { dotnetWebApi, SCAFFOLD_PROVIDERS, type EfAction, type EfModel, type EfRequest, type Migration } from "@/ipc/dotnetWeb";
import { openMenu } from "@/features/run/PopupMenu";
import { askConfirm } from "@/features/contextMenu/dialogs";
import { openFileAt } from "@/editor/views";
import { suspendKeymap } from "@/keymap";
import { isDialogWindow } from "@/shell/window/mode";
import { getSetting } from "@/state/settings";
import { onFileSaved } from "@/state/editor";
import { dotnetActive, dotnetWorkspace, errorText, runDotnetCapture, runDotnetCommand, showDotnetBalloon } from "../state";

const [model, { refetch: refetchModel }] = createRoot(() => createResource(
  () => (dotnetActive() ? dotnetWorkspace() : null),
  () => dotnetWebApi.efModel().catch(() => null as EfModel | null),
));
/** EF Core is used: a project references it. */
export const efActive = () => dotnetActive() && (model()?.projects.length ?? 0) > 0;

/** Applied state from the last `migrations list`, by migration id. */
const [applied, setApplied] = createSignal<Record<string, boolean>>({});
const [listed, setListed] = createSignal<Migration[]>([]);
const [busy, setBusy] = createSignal<string | null>(null);
const [selected, setSelected] = createSignal<string | null>(null);

if (typeof document !== "undefined")
  onFileSaved((path) => {
    if (/\.cs$/i.test(path) && efActive() && /Migrations?\/|Context\.cs$/i.test(path)) void refetchModel();
  });

/** Sources' migrations merged with the listed ones (applied state, database-only ones). */
const migrations = createRoot(() => createMemo<Migration[]>(() => {
  const src = model()?.migrations ?? [];
  const out: Migration[] = src.map((m) => ({ ...m, applied: applied()[m.id] ?? m.applied ?? null }));
  for (const l of listed()) if (!out.some((m) => m.id === l.id)) out.push(l);
  return out.sort((a, b) => a.id.localeCompare(b.id));
}));

const timestamp = (id: string) => {
  const t = id.split("_")[0] ?? "";
  return /^\d{14}$/.test(t) ? `${t.slice(0, 4)}-${t.slice(4, 6)}-${t.slice(6, 8)} ${t.slice(8, 10)}:${t.slice(10, 12)}` : "";
};

/** The project with the DbContext and the startup project of `dotnet ef`. */
function defaults(): Pick<EfRequest, "project" | "startupProject" | "context"> {
  const m = model();
  const ctx = m?.contexts[0];
  const project = ctx?.project ?? m?.projects[0]?.path ?? "";
  const configured = getSetting<string>("dotnet.ef.startupProject", "");
  const startup = configured || m?.projects.find((p) => p.runnable && p.design)?.path || m?.projects.find((p) => p.runnable)?.path || project;
  return { project, startupProject: startup, context: (m?.contexts.length ?? 0) > 1 ? ctx?.name : undefined };
}

async function offerToolInstall(): Promise<void> {
  showDotnetBalloon("dotnet-ef is not installed", "EF Core commands need the dotnet-ef tool (dotnet tool install --global dotnet-ef).", 15000, [
    {
      label: "Install dotnet-ef",
      run: async () => {
        if (!(await askConfirm({ title: "Install dotnet-ef", message: "Run 'dotnet tool install --global dotnet-ef' from nuget.org?", confirm: "Install" }))) return;
        const cmd = await dotnetWebApi.efCommand({ action: "installTool" });
        await runDotnetCommand(cmd, "Install dotnet-ef");
      },
    },
  ]);
}

/** `dotnet ef migrations list`: the applied state of every migration. */
export async function listMigrations(): Promise<void> {
  if (!efActive()) return;
  setBusy("Reading migrations from the database…");
  try {
    const cmd = await dotnetWebApi.efCommand({ action: "listMigrations", ...defaults() });
    const { output } = await runDotnetCapture(cmd, "EF Core: migrations list");
    const r = await dotnetWebApi.efOutput(output);
    if (r.toolMissing) return void offerToolInstall();
    if (r.error) {
      showDotnetBalloon("dotnet ef migrations list failed", r.hint ? `${r.error}\n${r.hint}` : r.error, 12000);
      return;
    }
    setListed(r.migrations);
    setApplied(Object.fromEntries(r.migrations.map((m) => [m.id, !!m.applied])));
  } catch (e) {
    showDotnetBalloon("Cannot list migrations", errorText(e), 8000);
  } finally {
    setBusy(null);
  }
}

/** Run an EF Core action, then refresh the sources and the applied state. */
export async function runEf(request: EfRequest, title: string): Promise<boolean> {
  try {
    const cmd = await dotnetWebApi.efCommand(request);
    setBusy(`${title}…`);
    const { code, output } = await runDotnetCapture(cmd, `EF Core: ${title}`);
    if (code !== 0) {
      const r = await dotnetWebApi.efOutput(output);
      if (r.toolMissing) void offerToolInstall();
      else showDotnetBalloon(`${title} failed`, r.hint ?? r.error ?? "See the Run tool window for dotnet ef's output.", 12000);
      return false;
    }
    await refetchModel();
    if (request.action !== "scaffold") void listMigrations();
    showDotnetBalloon(title, "Done", 4000);
    return true;
  } catch (e) {
    showDotnetBalloon(`Cannot run ${title}`, errorText(e), 8000);
    return false;
  } finally {
    setBusy(null);
  }
}

// ---- Dialog ---------------------------------------------------------------------------------------

type DialogMode = "addMigration" | "updateDatabase" | "scaffold" | "removeMigration";
const [dialog, setDialog] = createSignal<{ mode: DialogMode; target?: string } | null>(null);

const needsEf = (run: () => void) => () => {
  if (!efActive()) {
    showDotnetBalloon("No EF Core project", "No project of the solution references Microsoft.EntityFrameworkCore.", 6000);
    return;
  }
  run();
};

export const openAddMigration = needsEf(() => setDialog({ mode: "addMigration" }));
export const openUpdateDatabase = needsEf(() => setDialog({ mode: "updateDatabase" }));
export const openRemoveMigration = needsEf(() => setDialog({ mode: "removeMigration" }));
export const openScaffold = () => (dotnetActive() ? setDialog({ mode: "scaffold" }) : undefined);

const TITLES: Record<DialogMode, string> = {
  addMigration: "Add Migration",
  updateDatabase: "Update Database",
  scaffold: "Scaffold DbContext",
  removeMigration: "Remove Last Migration",
};

function EfDialog(props: { mode: DialogMode; target?: string }) {
  const release = suspendKeymap();
  onCleanup(release);
  const d = defaults();
  const allProjects = () => dotnetWorkspace()?.projects.filter((p) => !p.isTestProject) ?? [];
  const efProjects = () => (props.mode === "scaffold" ? allProjects().map((p) => ({ path: p.path, name: p.name })) : (model()?.projects ?? []));
  const [project, setProject] = createSignal(d.project || efProjects()[0]?.path || "");
  const [startup, setStartup] = createSignal(d.startupProject ?? "");
  const [context, setContext] = createSignal(d.context ?? "");
  const [name, setName] = createSignal(props.target ?? "");
  const [outputDir, setOutputDir] = createSignal(props.mode === "scaffold" ? "Models" : "");
  const [connection, setConnection] = createSignal("");
  const [provider, setProvider] = createSignal(model()?.projects.find((p) => p.provider)?.provider ?? SCAFFOLD_PROVIDERS[0]!);
  const [tables, setTables] = createSignal("");
  const [force, setForce] = createSignal(false);
  const [annotations, setAnnotations] = createSignal(false);
  const contexts = () => (model()?.contexts ?? []).filter((c) => !project() || c.project === project());
  const runnable = () => allProjects().filter((p) => p.web || /exe$/i.test(p.outputType));
  const lastMigration = () => [...migrations()].reverse().find((m) => !project() || m.project === project());
  const close = () => setDialog(null);
  const valid = () => !!project() && (props.mode !== "addMigration" || /^\w+$/.test(name().trim())) && (props.mode !== "scaffold" || (!!connection().trim() && !!provider()));
  const submit = async () => {
    if (!valid()) return;
    const request: EfRequest = {
      action: props.mode as EfAction,
      project: project(),
      startupProject: startup() || undefined,
      context: context() || undefined,
      name: name().trim() || undefined,
      outputDir: outputDir().trim() || undefined,
      connection: connection().trim() || undefined,
      provider: props.mode === "scaffold" ? provider() : undefined,
      tables: tables()
        .split(/[,\s]+/)
        .filter(Boolean),
      force: force(),
      dataAnnotations: annotations(),
    };
    close();
    await runEf(request, props.mode === "updateDatabase" && name() ? `Update Database to ${name()}` : TITLES[props.mode]);
  };
  return (
    <div class="popup-backdrop modal" onMouseDown={(e) => e.target === e.currentTarget && close()}>
      <div
        class="dialog dn-ef-dialog"
        style={{ width: "min(560px, calc(100vw - 32px))" }}
        role="dialog"
        data-testid={`ef-dialog-${props.mode}`}
        onKeyDown={(e) => {
          if (e.key === "Escape") close();
          else if (e.key === "Enter" && (e.target as HTMLElement).tagName !== "SELECT") void submit();
          else return;
          e.preventDefault();
          e.stopPropagation();
        }}
      >
        <div class="dialog-title">{TITLES[props.mode]}</div>
        <div class="dialog-body dn-ef-form">
          <Show when={props.mode === "addMigration"}>
            <label class="form-label">Migration name</label>
            <input ref={(el) => queueMicrotask(() => el.focus())} value={name()} placeholder="AddReviews" spellcheck={false} data-testid="ef-migration-name" onInput={(e) => setName(e.currentTarget.value)} />
          </Show>
          <Show when={props.mode === "updateDatabase"}>
            <label class="form-label">Target migration</label>
            <select value={name()} onChange={(e) => setName(e.currentTarget.value)}>
              <option value="">Latest migration</option>
              <option value="0">0 (revert all migrations)</option>
              <For each={migrations().filter((m) => !project() || m.project === project() || !m.project)}>{(m) => <option value={m.name}>{m.name}</option>}</For>
            </select>
          </Show>
          <Show when={props.mode === "removeMigration"}>
            <div class="dn-ef-note">
              Removes <b>{lastMigration()?.name ?? "the last migration"}</b> and reverts the model snapshot.
              <Show when={lastMigration()?.applied}> It is applied to the database: Force also reverts it there.</Show>
            </div>
          </Show>
          <Show when={props.mode === "scaffold"}>
            <label class="form-label">Connection string</label>
            <input ref={(el) => queueMicrotask(() => el.focus())} class="mono" value={connection()} placeholder="Host=localhost;Database=bookstore;Username=app" spellcheck={false} data-testid="ef-connection" onInput={(e) => setConnection(e.currentTarget.value)} />
            <label class="form-label">Provider</label>
            <select value={provider()} onChange={(e) => setProvider(e.currentTarget.value)}>
              <For each={SCAFFOLD_PROVIDERS}>{(p) => <option value={p}>{p}</option>}</For>
            </select>
            <label class="form-label">Tables</label>
            <input value={tables()} placeholder="All tables (or: books, authors)" spellcheck={false} onInput={(e) => setTables(e.currentTarget.value)} />
          </Show>
          <label class="form-label">{props.mode === "scaffold" ? "Target project" : "DbContext project"}</label>
          <select value={project()} onChange={(e) => setProject(e.currentTarget.value)}>
            <For each={efProjects()}>{(p) => <option value={p.path}>{p.name}</option>}</For>
          </select>
          <label class="form-label">Startup project</label>
          <select value={startup()} onChange={(e) => setStartup(e.currentTarget.value)}>
            <option value="">Same as the project</option>
            <For each={runnable()}>{(p) => <option value={p.path}>{p.name}</option>}</For>
          </select>
          <label class="form-label">DbContext</label>
          <Show
            when={props.mode !== "scaffold" && contexts().length}
            fallback={<input value={context()} placeholder={props.mode === "scaffold" ? "BookstoreContext (from the database name)" : "The only DbContext"} spellcheck={false} onInput={(e) => setContext(e.currentTarget.value)} />}
          >
            <select value={context()} onChange={(e) => setContext(e.currentTarget.value)}>
              <Show when={contexts().length === 1}>
                <option value="">{contexts()[0]!.name}</option>
              </Show>
              <For each={contexts().length > 1 ? contexts() : []}>{(c) => <option value={c.name}>{c.name}</option>}</For>
            </select>
          </Show>
          <Show when={props.mode === "addMigration" || props.mode === "scaffold"}>
            <label class="form-label">Output folder</label>
            <input value={outputDir()} placeholder="Migrations" spellcheck={false} onInput={(e) => setOutputDir(e.currentTarget.value)} />
          </Show>
          <Show when={props.mode === "scaffold" || props.mode === "removeMigration"}>
            <span />
            <label class="dn-check">
              <input type="checkbox" checked={force()} onChange={(e) => setForce(e.currentTarget.checked)} />
              {props.mode === "scaffold" ? "Overwrite existing files (--force)" : "Revert it in the database too (--force)"}
            </label>
          </Show>
          <Show when={props.mode === "scaffold"}>
            <span />
            <label class="dn-check">
              <input type="checkbox" checked={annotations()} onChange={(e) => setAnnotations(e.currentTarget.checked)} />
              Use data annotations
            </label>
          </Show>
        </div>
        <div class="dialog-buttons">
          <span class="muted dn-ef-hint">Runs dotnet ef in the Run tool window</span>
          <button class="btn-secondary" onClick={close}>
            Cancel
          </button>
          <button class="btn-primary" disabled={!valid()} onClick={() => void submit()} data-testid="ef-submit">
            {props.mode === "removeMigration" ? "Remove" : props.mode === "updateDatabase" ? "Update" : props.mode === "scaffold" ? "Scaffold" : "Add"}
          </button>
        </div>
      </div>
    </div>
  );
}

if (typeof document !== "undefined" && !isDialogWindow()) {
  const host = document.createElement("div");
  host.id = "dotnet-ef-overlay";
  document.body.appendChild(host);
  render(() => <Show when={dialog()}>{(d) => <EfDialog mode={d().mode} target={d().target} />}</Show>, host);
}

// ---- Tool window ---------------------------------------------------------------------------------------

function migrationMenu(m: Migration, ev: MouseEvent) {
  ev.preventDefault();
  openMenu({
    x: ev.clientX,
    y: ev.clientY,
    items: [
      { label: "Update Database to This Migration", icon: DatabaseZap, run: () => setDialog({ mode: "updateDatabase", target: m.name }) },
      { label: "Go to Migration", icon: FileCode, disabled: !m.file, run: () => m.file && void openFileAt(m.file, m.line + 1) },
      { separator: true },
      { label: "Copy Migration Id", run: () => void navigator.clipboard?.writeText(m.id).catch(() => undefined) },
    ],
  });
}

function Status(props: { applied?: boolean | null }) {
  return (
    <Show when={props.applied != null} fallback={<span class="muted dn-ef-status" title="Refresh to read the database">—</span>}>
      <span class="dn-ef-status" classList={{ applied: !!props.applied, pending: !props.applied }}>
        {props.applied ? <Check size={12} /> : <Clock size={12} />}
        {props.applied ? "Applied" : "Pending"}
      </span>
    </Show>
  );
}

export function EfCoreToolWindow() {
  const projectName = (path?: string | null) => dotnetWorkspace()?.projects.find((p) => p.path === path)?.name ?? path?.split("/").pop() ?? "";
  return (
    <div class="dn-tw" data-testid="efcore-window">
      <div class="dn-toolbar">
        <button class="icon-btn" title="Refresh Migrations (dotnet ef migrations list)" onClick={() => void listMigrations()} disabled={!!busy()}>
          <RefreshCw size={14} class={busy() ? "dn-spin" : undefined} />
        </button>
        <span class="dn-sep" />
        <button class="icon-btn" title="Add Migration…" onClick={openAddMigration} data-testid="ef-add">
          <Plus size={14} />
        </button>
        <button class="icon-btn" title="Remove Last Migration…" onClick={openRemoveMigration} disabled={!migrations().length}>
          <Minus size={14} />
        </button>
        <button class="icon-btn" title="Update Database…" onClick={openUpdateDatabase}>
          <DatabaseZap size={14} />
        </button>
        <button class="icon-btn" title="Scaffold DbContext…" onClick={openScaffold}>
          <Wand2 size={14} />
        </button>
        <span class="dn-spacer" />
        <span class="muted dn-ef-busy">{busy() ?? (model()?.localTool ? "dotnet-ef (local tool)" : "")}</span>
      </div>
      <div class="dn-split dn-ef-split">
        <div class="dn-ef-contexts">
          <div class="dn-ef-head">DbContexts</div>
          <For each={model()?.contexts ?? []} fallback={<div class="muted dn-message">No DbContext classes in the sources</div>}>
            {(c) => (
              <div class="dn-ef-context">
                <div class="dn-ef-row" onClick={() => void openFileAt(c.file, c.line + 1)} title={c.file}>
                  <Database size={13} class="dn-ef-db" />
                  <span>{c.name}</span>
                  <span class="muted dn-detail">{projectName(c.project)}</span>
                </div>
                <For each={c.sets}>
                  {(s) => (
                    <div class="dn-ef-row dn-ef-set">
                      <Table2 size={12} class="muted" />
                      <span>{s.name}</span>
                      <span class="muted dn-detail">{s.entity}</span>
                    </div>
                  )}
                </For>
              </div>
            )}
          </For>
          <For each={model()?.projects ?? []}>
            {(p) => (
              <div class="muted dn-ef-provider" title={p.design ? "" : "Add Microsoft.EntityFrameworkCore.Design to run dotnet ef from this project"}>
                {p.name}: {p.provider?.replace(/\.EntityFrameworkCore\.?/, " ") ?? "no provider"}
                {p.version ? ` ${p.version}` : ""}
              </div>
            )}
          </For>
        </div>
        <div class="dn-ef-migrations">
          <table class="dn-ep-table">
            <thead>
              <tr>
                <th>Migration</th>
                <th>Created</th>
                <th>DbContext</th>
                <th>Project</th>
                <th>Status</th>
              </tr>
            </thead>
            <tbody>
              <For each={migrations()}>
                {(m) => (
                  <tr
                    classList={{ selected: selected() === m.id }}
                    data-migration={m.name}
                    onClick={() => (setSelected(m.id), m.file && void openFileAt(m.file, m.line + 1))}
                    onContextMenu={(ev) => (setSelected(m.id), migrationMenu(m, ev))}
                  >
                    <td>
                      <span class="dn-ef-name">
                        <Show when={m.file} fallback={<CircleDashed size={12} class="muted" />}>
                          <FileCode size={12} class="muted" />
                        </Show>
                        {m.name}
                      </span>
                    </td>
                    <td class="muted">{timestamp(m.id)}</td>
                    <td>{m.context ?? ""}</td>
                    <td class="muted">{projectName(m.project)}</td>
                    <td>
                      <Status applied={m.applied} />
                    </td>
                  </tr>
                )}
              </For>
            </tbody>
          </table>
          <Show when={!migrations().length}>
            <div class="dn-ep-empty">
              <DatabaseZap size={22} />
              <div>No migrations yet: Add Migration creates the first one</div>
            </div>
          </Show>
        </div>
      </div>
    </div>
  );
}

export const refreshEfModel = () => void refetchModel();
