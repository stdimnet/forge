/**
 * Settings | Languages & Frameworks | Language Servers: every server Forge knows, grouped
 * by language, with status, detected path, enable toggle, Install… and per-server
 * options (custom path, extra JSON settings). Edits are drafts until Apply, like every
 * settings page.
 */
import { createMemo, createSignal, For, onMount, Show } from "solid-js";
import { Download, ExternalLink, LoaderCircle, RefreshCw, Search } from "lucide-solid";
import { serverKey, type ServerRow } from "@/ipc/langservers";
import { settingsState } from "@/state/settings";
import { workspace } from "@/state/workspace";
import { draft, draftValue, openSettings, setDraft } from "@/features/settings/dialogState";
import { SettingRow } from "@/features/settings/SchemaPage";
import { InstallDialog } from "./InstallDialog";
import { IntelephenseLicence } from "./Licence";
import { groupRows, jsonError, shortenHome, skipText, STATUS_LABEL } from "./model";
import { applyServers, loading, refreshRows, rows } from "./store";
import { managedOption, progressText } from "./autoInstall";
import { installFor, installManaged } from "./managed";

/**
 * Install button of a row: Forge's own installer when it can do it (npm servers: one click,
 * progress inline), otherwise the dialog with the commands.
 */
function InstallButton(props: { row: ServerRow; onDialog: () => void }) {
  const running = () => installFor(props.row.id);
  return (
    <Show
      when={!running()}
      fallback={
        <span class="ls-installing" title={running()!.label}>
          <LoaderCircle size={13} class="lsp-spin" />
          {progressText(running()!.label, running()!.done, running()!.total)}
        </span>
      }
    >
      <Show
        when={managedOption(props.row)}
        fallback={
          <button
            class="btn-secondary ls-small-btn"
            onClick={(e) => {
              e.stopPropagation();
              props.onDialog();
            }}
          >
            <Download size={13} /> Install…
          </button>
        }
      >
        <button
          class="btn-secondary ls-small-btn"
          data-testid={`install-${props.row.id}`}
          title="Forge downloads it into its own server directory (and Node.js, if needed)"
          onClick={(e) => {
            e.stopPropagation();
            void installManaged(props.row);
          }}
        >
          <Download size={13} /> Install
        </button>
      </Show>
    </Show>
  );
}

/** Draft-aware value of a per-server setting. */
function serverValue<T>(id: string, field: "enabled" | "path" | "settings", fallback: T): T {
  const key = serverKey(id, field);
  const v = key in draft ? draft[key] : draftValue<never>(key);
  return (v ?? fallback) as T;
}

/** First section of the server's default settings, as an example key for the editor. */
function settingsSection(row: ServerRow): string {
  const settings = row.defaults.settings;
  const first = settings && typeof settings === "object" ? Object.keys(settings)[0] : undefined;
  return first ?? "settings";
}

function StatusChip(props: { row: ServerRow; enabled: boolean }) {
  const status = () => (props.enabled ? (props.row.status === "disabled" ? (props.row.path ? "installed" : "notInstalled") : props.row.status) : "disabled");
  return (
    <span class={`ls-chip ${status()}`} title={props.row.detail ?? undefined}>
      <span class="ls-dot" />
      {STATUS_LABEL[status()]}
      <Show when={status() === "running" && props.row.detail}>
        <span class="ls-chip-detail">· {props.row.detail}</span>
      </Show>
    </span>
  );
}

function Details(props: { row: ServerRow; onInstall: () => void }) {
  const row = () => props.row;
  const isPack = () => row().source !== "catalog";
  const extra = () => serverValue<string>(row().id, "settings", row().extraSettings ?? "");
  const extraError = () => jsonError(extra());
  const [showDefaults, setShowDefaults] = createSignal(false);
  const skip = () => skipText(row().skip, rows());
  return (
    <div class="ls-details" data-testid="ls-details">
      <div class="ls-details-head">
        <span class="ls-details-name">{row().name}</span>
        <span class="muted">{row().languages.join(", ")}</span>
        <span class="ls-spacer" />
        <a class="ls-link" href={row().homepage} target="_blank" rel="noreferrer noopener">
          {row().license} · Homepage <ExternalLink size={12} />
        </a>
      </div>
      <Show when={row().note}>
        <p class="ls-note">{row().note}</p>
      </Show>
      <div class="ls-form">
        <span class="ls-form-label">Command:</span>
        <code class="ls-code">{[row().command, ...row().args].join(" ")}</code>

        <Show when={!isPack()}>
          <label class="ls-form-label" for="ls-custom-path">
            Executable:
          </label>
          <input
            id="ls-custom-path"
            class="ls-input ls-path-input"
            spellcheck={false}
            placeholder={row().path && !row().customPath ? `Detected: ${shortenHome(row().path)}` : "Detect automatically"}
            value={serverValue<string>(row().id, "path", row().customPath ?? "")}
            onInput={(e) => setDraft(serverKey(row().id, "path"), e.currentTarget.value.trim())}
          />
        </Show>
        <Show when={isPack()}>
          <span class="ls-form-label">Executable:</span>
          <span class="ls-path-text">
            {row().builtinVersion ? `Built into Forge · version ${row().builtinVersion}` : row().path ? shortenHome(row().path) : "Not found"}
            <span class="muted"> · configured in </span>
            <button class="ls-link" onClick={() => openSettings("lang.php")}>
              Languages & Frameworks | PHP
            </button>
          </span>
        </Show>

        <Show when={skip() && row().enabled && row().status !== "running"}>
          <span class="ls-form-label">Project:</span>
          <span class="muted">{skip()}</span>
        </Show>

        <Show when={row().status === "notInstalled" || (row().install.length > 0 && !row().path)}>
          <span class="ls-form-label">Install:</span>
          <span class="ls-install-line">
            <Show when={managedOption(row())} fallback={<code class="ls-code">{row().install[0]?.command}</code>}>
              <span class="muted">From the npm registry into Forge's server directory, with Node.js if needed.</span>
            </Show>
            <span class="ls-install-actions">
              <InstallButton row={row()} onDialog={props.onInstall} />
              <Show when={managedOption(row()) && row().install.length > 1}>
                <button class="ls-link" onClick={props.onInstall}>
                  Other ways…
                </button>
              </Show>
            </span>
          </span>
        </Show>

        <Show when={row().id === "intelephense"}>
          <span class="ls-form-label">Licence:</span>
          <IntelephenseLicence compact />
        </Show>
        <Show when={row().id === "lua-language-server"}>
          <span class="ls-form-label">Environment:</span>
          <span>
            <button class="ls-link" onClick={() => openSettings("lang.lua")}>
              Lua runtime, LÖVE and Neovim APIs…
            </button>
          </span>
        </Show>
      </div>

      <Show when={!isPack()}>
        <div class="ls-json-head">
          <span>Extra settings (JSON):</span>
          <span class="ls-spacer" />
          <button class="ls-link" onClick={() => setShowDefaults((v) => !v)}>
            {showDefaults() ? "Hide Defaults" : "Show Defaults"}
          </button>
        </div>
        <Show when={showDefaults()}>
          <pre class="ls-defaults">{JSON.stringify(row().defaults, null, 2)}</pre>
        </Show>
        <textarea
          class="ls-json"
          classList={{ invalid: !!extraError() }}
          spellcheck={false}
          placeholder={`{\n  "${settingsSection(row())}": { }\n}`}
          value={extra()}
          onInput={(e) => setDraft(serverKey(row().id, "settings"), e.currentTarget.value)}
        />
        <Show
          when={extraError()}
          fallback={
            <p class="ls-hint">
              Merged into the server's settings. Put startup options under an "initializationOptions" key.
            </p>
          }
        >
          <div class="ls-error">{extraError()}</div>
        </Show>
      </Show>
    </div>
  );
}

/**
 * `groups` limits the page to some catalog groups (e.g. "Vue"), for language pages that embed
 * the servers of their languages.
 */
export function LanguageServersPage(props: { groups?: string[] } = {}) {
  const [filter, setFilter] = createSignal("");
  const [projectOnly, setProjectOnly] = createSignal(false);
  const [selectedId, setSelectedId] = createSignal<string | null>(null);
  const [installing, setInstalling] = createSignal<ServerRow | null>(null);
  onMount(() => void refreshRows());

  const ownRows = createMemo(() => (props.groups ? rows().filter((r) => props.groups!.includes(r.group)) : rows()));
  const groups = createMemo(() => groupRows(ownRows(), { filter: filter(), projectOnly: projectOnly() }));
  const selected = createMemo(() => ownRows().find((r) => r.id === selectedId()) ?? groups()[0]?.rows[0]);
  const generalSpecs = createMemo(() =>
    settingsState.schema.filter((s) => s.group === "lang.servers").sort((a, b) => a.order - b.order),
  );
  const enabled = (r: ServerRow) => serverValue<boolean>(r.id, "enabled", r.enabled);
  const counts = createMemo(() => {
    const all = ownRows();
    return {
      running: all.filter((r) => r.status === "running").length,
      installed: all.filter((r) => r.path).length,
      total: all.length,
    };
  });

  return (
    <div class="ls-page" data-testid="language-servers-page">
      <Show when={!props.groups}>
      <p class="ls-intro muted">
        Language servers are separate programs. Forge looks in the project's node_modules, in its own server directory
        (~/.local/share/forge/langservers), on PATH and in pip, cargo, go and Mason directories. Install puts npm
        servers into that directory only, never globally, and needs nothing else: Forge downloads Node.js too when
        it is missing.
      </p>
      </Show>
      <Show when={workspace()?.remote}>
        {(r) => (
          <p class="ls-remote-note" data-testid="ls-remote-note">
            This project is on <b>{r().label}</b>: its language servers run there, the list shows what is installed on
            that host, and Install downloads Node.js and the servers into Forge's directory on the host.
          </p>
        )}
      </Show>
      <Show when={generalSpecs().length}>
        <div class="set-section-body ls-general">
          <For each={generalSpecs()}>{(spec) => <SettingRow spec={spec} />}</For>
        </div>
      </Show>

      <div class="ls-toolbar">
        <div class="ls-filter">
          <Search size={13} class="muted" />
          <input
            placeholder="Filter servers or languages"
            spellcheck={false}
            value={filter()}
            onInput={(e) => setFilter(e.currentTarget.value)}
          />
        </div>
        <label class="set-check ls-project-only">
          <input type="checkbox" checked={projectOnly()} onChange={(e) => setProjectOnly(e.currentTarget.checked)} />
          <span>Project languages only</span>
        </label>
        <span class="ls-spacer" />
        <span class="ls-counts muted">
          {counts().running} running · {counts().installed} installed
        </span>
        <button class="icon-btn" title="Detect Again" onClick={() => void applyServers()}>
          <RefreshCw size={14} classList={{ "lsp-spin": loading() }} />
        </button>
      </div>

      <div class="ls-table" role="table" aria-label="Language servers">
        <div class="ls-tr ls-th" role="row">
          <span>Server</span>
          <span>Status</span>
          <span>Path</span>
        </div>
        <div class="ls-tbody">
          <For each={groups()} fallback={<div class="ls-empty muted">{loading() ? "Detecting language servers…" : "Nothing to show"}</div>}>
            {(g) => (
              <>
                <div class="ls-group" role="row">
                  {g.title}
                  <Show when={g.rows.some((r) => r.inProject)}>
                    <span class="ls-group-hint">used in this project</span>
                  </Show>
                </div>
                <For each={g.rows}>
                  {(r) => (
                    <div
                      class="ls-tr"
                      role="row"
                      data-server={r.id}
                      classList={{ selected: selected()?.id === r.id, off: !enabled(r) }}
                      onClick={() => setSelectedId(r.id)}
                    >
                      <span class="ls-name-cell">
                        <input
                          type="checkbox"
                          title={r.source === "catalog" ? "Enabled" : "Chosen in Languages & Frameworks | PHP"}
                          checked={enabled(r)}
                          disabled={r.source !== "catalog"}
                          onClick={(e) => e.stopPropagation()}
                          onChange={(e) => setDraft(serverKey(r.id, "enabled"), e.currentTarget.checked)}
                        />
                        <span class="ls-name">{r.name}</span>
                      </span>
                      <span>
                        <StatusChip row={r} enabled={enabled(r)} />
                      </span>
                      <span class="ls-path-cell">
                        <Show
                          when={!r.builtinVersion}
                          fallback={
                            <span class="ls-path ls-builtin" title={r.path ?? ""} data-testid="ls-builtin">
                              Built-in · {r.builtinVersion}
                            </span>
                          }
                        >
                          <Show
                            when={r.path}
                            fallback={
                              <Show when={r.install.length}>
                                <InstallButton row={r} onDialog={() => setInstalling(r)} />
                              </Show>
                            }
                          >
                            <span class="ls-path" title={r.path ?? ""}>
                              {shortenHome(r.path)}
                            </span>
                          </Show>
                        </Show>
                      </span>
                    </div>
                  )}
                </For>
              </>
            )}
          </For>
        </div>
      </div>

      <Show when={selected()}>{(row) => <Details row={row()} onInstall={() => setInstalling(row())} />}</Show>

      <Show when={installing()}>{(row) => <InstallDialog row={row()} onClose={() => setInstalling(null)} />}</Show>
    </div>
  );
}
