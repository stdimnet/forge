/**
 * Settings | Tools | Actions on Save: one row per action with a checkbox, its options or
 * the tool's location, and a "Configure…" link to the page that configures it. Rows of
 * disabled plugins are left out (their settings are not in the schema). Values go through
 * the dialog draft, so Apply, Cancel and Reset work.
 */
import { createResource, createSignal, For, Show, type JSX } from "solid-js";
import { ChevronDown } from "lucide-solid";
import { saveActionsApi, type SaveActionsInfo, type SaveTool } from "@/ipc/saveActions";
import { findSpec } from "@/state/settings";
import { shortcutLabel } from "@/keymap";
import { draft, draftValue, openSettings, setDraft } from "@/features/settings/dialogState";
import { Hl, settingMatches } from "@/features/settings/dialogSearch";
import { TOOL_PAGES } from "./run";

const K = {
  reformat: "actionsOnSave.reformat.enabled",
  scope: "actionsOnSave.reformat.scope",
  fileTypes: "actionsOnSave.reformat.fileTypes",
  optimizeImports: "actionsOnSave.optimizeImports",
  rearrange: "actionsOnSave.rearrange",
  cleanup: "actionsOnSave.codeCleanup",
  eslint: "actionsOnSave.eslint",
  prettier: "actionsOnSave.prettier.enabled",
  prettierFiles: "actionsOnSave.prettier.files",
  phpCsFixer: "actionsOnSave.phpCsFixer",
  pint: "actionsOnSave.pint",
  phpcbf: "actionsOnSave.phpcbf",
  rector: "actionsOnSave.rector",
  autosave: "actionsOnSave.runOnAutosave",
};

interface Row {
  key: string;
  title: string;
  /** Options or status shown right of the title. */
  extra?: (enabled: () => boolean) => JSX.Element;
  configure?: { page: string; title: string };
}

/** Where a tool was found, or that it is missing. */
function ToolStatus(props: { info: SaveActionsInfo | undefined; tool: SaveTool }) {
  const location = () => props.info?.tools.find((t) => t.tool === props.tool);
  return (
    <Show when={props.info}>
      <Show
        when={location()?.path}
        fallback={<span class="aos-missing">Not installed in the project</span>}
      >
        {(path) => <span class="aos-muted" title={path()}>{path()}</span>}
      </Show>
    </Show>
  );
}

/** "All file types ▾": the file types reformatted on save. */
function FileTypePicker(props: { info: SaveActionsInfo | undefined; disabled: boolean }) {
  const [open, setOpen] = createSignal(false);
  const selected = () => draftValue<string[]>(K.fileTypes) ?? [];
  const label = () => {
    const ids = selected();
    if (!ids.length) return "All file types";
    const names = ids.map((id) => props.info?.fileTypes.find((t) => t.id === id)?.name ?? id);
    return names.length > 2 ? `${names.slice(0, 2).join(", ")} +${names.length - 2}` : names.join(", ");
  };
  const toggle = (id: string, on: boolean) =>
    setDraft(K.fileTypes, on ? [...selected(), id] : selected().filter((x) => x !== id));
  return (
    <span class="aos-picker">
      <button class="set-link aos-dropdown" disabled={props.disabled} onClick={() => setOpen(!open())}>
        {label()}
        <ChevronDown size={12} />
      </button>
      <Show when={open()}>
        <div class="aos-popup" onMouseLeave={() => setOpen(false)}>
          <label class="set-check aos-popup-item">
            <input type="checkbox" checked={!selected().length} onChange={() => setDraft(K.fileTypes, [])} />
            <span>All file types</span>
          </label>
          <div class="aos-popup-sep" />
          <For each={props.info?.fileTypes ?? []}>
            {(t) => (
              <label class="set-check aos-popup-item">
                <input
                  type="checkbox"
                  checked={selected().includes(t.id)}
                  onChange={(e) => toggle(t.id, e.currentTarget.checked)}
                />
                <span>{t.name}</span>
              </label>
            )}
          </For>
        </div>
      </Show>
    </span>
  );
}

export function ActionsOnSavePage() {
  const [info] = createResource(() => saveActionsApi.info().catch(() => undefined));
  const saveKey = () => shortcutLabel("SaveAll") || "Ctrl+S";
  const rows: Row[] = [
    {
      key: K.reformat,
      title: "Reformat code",
      extra: (enabled) => (
        <>
          <FileTypePicker info={info()} disabled={!enabled()} />
          <select
            class="set-input set-select aos-select"
            disabled={!enabled()}
            value={draftValue<string>(K.scope)}
            onChange={(e) => setDraft(K.scope, e.currentTarget.value)}
          >
            <option value="whole">Whole file</option>
            <option value="changed">Changed lines</option>
          </select>
        </>
      ),
      configure: { page: "editor.code-style", title: "Code Style" },
    },
    {
      key: K.optimizeImports,
      title: "Optimize imports",
      extra: () => <span class="aos-muted">PHP: remove unused, sort</span>,
    },
    {
      key: K.rearrange,
      title: "Rearrange code",
      extra: () => <span class="aos-muted">PHP: constants, properties, constructor, methods</span>,
    },
    {
      key: K.cleanup,
      title: "Run code cleanup",
      extra: () => <span class="aos-muted">PHP: unused imports, unnecessary semicolons, closing tag</span>,
    },
    {
      key: K.eslint,
      title: "Run eslint --fix",
      extra: () => <ToolStatus info={info()} tool="eslint" />,
      configure: { page: TOOL_PAGES.eslint, title: "ESLint" },
    },
    {
      key: K.prettier,
      title: "Run Prettier",
      extra: (enabled) => (
        <>
          <input
            class="set-input aos-files"
            spellcheck={false}
            disabled={!enabled()}
            title="Run for files"
            value={draftValue<string>(K.prettierFiles)}
            onInput={(e) => setDraft(K.prettierFiles, e.currentTarget.value)}
          />
          <ToolStatus info={info()} tool="prettier" />
        </>
      ),
      configure: { page: TOOL_PAGES.prettier, title: "Prettier" },
    },
    {
      key: K.rector,
      title: "Run Rector",
      extra: () => <ToolStatus info={info()} tool="rector" />,
      configure: { page: TOOL_PAGES.rector, title: "Quality Tools" },
    },
    {
      key: K.phpCsFixer,
      title: "Run PHP CS Fixer",
      extra: () => <ToolStatus info={info()} tool="phpCsFixer" />,
      configure: { page: TOOL_PAGES.phpCsFixer, title: "Quality Tools" },
    },
    {
      key: K.phpcbf,
      title: "Run phpcbf",
      extra: () => <ToolStatus info={info()} tool="phpcbf" />,
      configure: { page: TOOL_PAGES.phpcbf, title: "Quality Tools" },
    },
    {
      key: K.pint,
      title: "Run Laravel Pint",
      extra: () => <ToolStatus info={info()} tool="pint" />,
      configure: { page: TOOL_PAGES.pint, title: "Quality Tools" },
    },
  ];
  const visible = () => rows.filter((r) => findSpec(r.key));
  const hit = (key: string) =>
    settingMatches(key) ||
    (key === K.reformat && (settingMatches(K.scope) || settingMatches(K.fileTypes))) ||
    (key === K.prettier && settingMatches(K.prettierFiles));
  const modified = (key: string) =>
    key in draft ||
    (key === K.reformat && (K.scope in draft || K.fileTypes in draft)) ||
    (key === K.prettier && K.prettierFiles in draft);

  return (
    <div class="set-page aos-page">
      <div class="set-desc aos-intro">
        Actions run when files are saved explicitly ({saveKey()}). External tools come from the project
        (vendor/bin, node_modules/.bin) and run where the project lives.
      </div>
      <div class="aos-list">
        <For each={visible()}>
          {(row) => {
            const enabled = () => draftValue<boolean>(row.key) === true;
            return (
              <div class="aos-row" classList={{ hit: hit(row.key), modified: modified(row.key) }} data-setting={row.key}>
                <label class="set-check aos-check">
                  <input type="checkbox" checked={enabled()} onChange={(e) => setDraft(row.key, e.currentTarget.checked)} />
                  <span>
                    <Hl text={row.title} />
                  </span>
                </label>
                <div class="aos-extra" classList={{ off: !enabled() }}>
                  {row.extra?.(enabled)}
                </div>
                <Show when={row.configure}>
                  {(c) => (
                    <button class="set-link aos-configure" title={`Open ${c().title}`} onClick={() => openSettings(c().page)}>
                      Configure…
                    </button>
                  )}
                </Show>
              </div>
            );
          }}
        </For>
      </div>
      <Show when={findSpec(K.autosave)}>
        <div class="set-row aos-autosave" classList={{ hit: settingMatches(K.autosave), modified: K.autosave in draft }} data-setting={K.autosave}>
          <label class="set-check">
            <input
              type="checkbox"
              checked={draftValue<boolean>(K.autosave)}
              onChange={(e) => setDraft(K.autosave, e.currentTarget.checked)}
            />
            <span>
              <Hl text="Also run on autosave" />
            </span>
          </label>
          <div class="set-indent set-desc">
            Reformat code, Optimize imports, Rearrange code and Run code cleanup also run when files are saved
            automatically (Settings | Appearance & Behavior | System Settings). External tools only run on explicit save.
          </div>
        </div>
      </Show>
    </div>
  );
}
