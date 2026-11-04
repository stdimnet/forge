/**
 * Settings | Tools | File Watchers, like PHPStorm's page: the project's watchers with a
 * check box each, "+" with the templates (SCSS, Sass, Less, TypeScript, Babel, Prettier,
 * Custom), remove, edit and copy, and the New / Edit Watcher dialog. The templates and
 * the validation come from Rust (`forge_runtools::watchers`); edits go to the settings
 * draft (Apply / Cancel).
 */
import { createResource, createSignal, For, Show } from "solid-js";
import { Copy, Eye, Minus, Pencil, Plus } from "lucide-solid";
import { draftValue, setDraft } from "@/features/settings/dialogState";
import { item, showContextMenu } from "@/features/contextMenu/state";
import { scopeEntries } from "@/features/scopes/model";
import {
  fileWatchersApi,
  watcherFromObject,
  watcherToObject,
  WATCHER_FILE_TYPES,
  type FileWatcher,
  type ShowConsole,
} from "@/ipc/webTooling";
import type { SettingValue } from "@/ipc/settings";
import { WATCHERS_KEY } from "./run";
import { t as tr } from "@/i18n";

const decode = (v: SettingValue | undefined): FileWatcher[] =>
  Array.isArray(v) ? (v as unknown as Record<string, unknown>[]).filter((o) => o && typeof o === "object").map(watcherFromObject) : [];
const encode = (list: FileWatcher[]): SettingValue => list.map(watcherToObject) as unknown as SettingValue;

const typeLabel = (id: string) => WATCHER_FILE_TYPES.find((t) => t.id === id)?.label ?? id;

export function FileWatchersPage() {
  const watchers = () => decode(draftValue(WATCHERS_KEY));
  const setWatchers = (next: FileWatcher[]) => setDraft(WATCHERS_KEY, encode(next));
  const [selected, setSelected] = createSignal<number | null>(watchers().length ? 0 : null);
  const [editing, setEditing] = createSignal<{ index: number; watcher: FileWatcher } | null>(null);
  const [templates] = createResource(() => fileWatchersApi.templates().catch(() => []));
  const current = () => {
    const i = selected();
    return i === null ? undefined : watchers()[i];
  };

  const add = (e: MouseEvent) => {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    showContextMenu(
      (templates() ?? []).map((t) =>
        item(t.label, () => {
          const taken = watchers().map((w) => w.name);
          let name = t.watcher.name;
          for (let n = 2; taken.includes(name); n++) name = `${t.watcher.name} ${n}`;
          setEditing({ index: -1, watcher: { ...t.watcher, name } });
        }),
      ),
      r.left,
      r.bottom + 2,
    );
  };
  const remove = () => {
    const i = selected();
    if (i === null) return;
    const rest = watchers().filter((_, j) => j !== i);
    setWatchers(rest);
    setSelected(rest.length ? Math.min(i, rest.length - 1) : null);
  };
  const copy = () => {
    const w = current();
    if (!w) return;
    const taken = watchers().map((x) => x.name);
    let name = `${w.name} (copy)`;
    for (let n = 2; taken.includes(name); n++) name = `${w.name} (copy ${n})`;
    setWatchers([...watchers(), { ...w, name, outputFilters: [...w.outputFilters] }]);
    setSelected(watchers().length - 1);
  };
  const finish = (w: FileWatcher | null) => {
    const e = editing();
    setEditing(null);
    if (!w || !e) return;
    if (e.index < 0) {
      setWatchers([...watchers(), w]);
      setSelected(watchers().length - 1);
    } else {
      setWatchers(watchers().map((x, i) => (i === e.index ? w : x)));
    }
  };

  return (
    <div class="fw-page">
      <div class="set-list fw-list">
        <div class="set-list-toolbar">
          <button class="icon-btn" title={tr("common.add")} aria-label={tr("common.add")} data-testid="fw-add" onClick={add}>
            <Plus size={14} />
          </button>
          <button class="icon-btn" title={tr("common.remove")} aria-label={tr("common.remove")} disabled={!current()} onClick={remove}>
            <Minus size={14} />
          </button>
          <button
            class="icon-btn"
            title={tr("common.edit")}
            aria-label={tr("common.edit")}
            disabled={!current()}
            onClick={() => current() && setEditing({ index: selected()!, watcher: current()! })}
          >
            <Pencil size={14} />
          </button>
          <button class="icon-btn" title={tr("common.copy")} aria-label={tr("common.copy")} disabled={!current()} onClick={copy}>
            <Copy size={14} />
          </button>
        </div>
        <table class="fw-table">
          <thead>
            <tr>
              <th class="fw-check" />
              <th>{tr("common.name")}</th>
              <th>{tr("langs.webTooling.fileType")}</th>
              <th>{tr("common.scope")}</th>
              <th>{tr("langs.webTooling.program")}</th>
            </tr>
          </thead>
          <tbody>
            <For
              each={watchers()}
              fallback={
                <tr>
                  <td colSpan={5} class="set-list-empty">
                    {tr("langs.webTooling.noFileWatchersClickTo")}
                  </td>
                </tr>
              }
            >
              {(w, i) => (
                <tr
                  classList={{ selected: selected() === i() }}
                  data-watcher={w.name}
                  onClick={() => setSelected(i())}
                  onDblClick={() => setEditing({ index: i(), watcher: w })}
                >
                  <td class="fw-check">
                    <input
                      type="checkbox"
                      checked={w.enabled}
                      onClick={(e) => e.stopPropagation()}
                      onChange={(e) => setWatchers(watchers().map((x, j) => (j === i() ? { ...x, enabled: e.currentTarget.checked } : x)))}
                    />
                  </td>
                  <td>
                    <Eye size={13} class="muted" /> {w.name}
                  </td>
                  <td>{typeLabel(w.fileType)}</td>
                  <td>{w.scope || tr("langs.webTooling.projectFiles")}</td>
                  <td class="fw-mono">{w.program}</td>
                </tr>
              )}
            </For>
          </tbody>
        </table>
      </div>
      <div class="set-desc fw-hint">
        {tr("langs.webTooling.watchersRunWhenAFile")}
      </div>
      <Show when={editing()}>
        {(e) => (
          <EditWatcherDialog
            watcher={e().watcher}
            creating={e().index < 0}
            others={watchers().filter((_, i) => i !== e().index)}
            onDone={finish}
          />
        )}
      </Show>
    </div>
  );
}

const MACROS = [
  "$FileName$",
  "$FileNameWithoutExtension$",
  "$FileNameWithoutAllExtensions$",
  "$FileDir$",
  "$FilePath$",
  "$FilePathRelativeToProjectRoot$",
  "$FileDirRelativeToProjectRoot$",
  "$ProjectFileDir$",
  "$FileExt$",
];

type TextField = "program" | "arguments" | "outputPaths" | "workingDirectory";

/** New Watcher / Edit Watcher. */
export function EditWatcherDialog(props: { watcher: FileWatcher; creating: boolean; others: FileWatcher[]; onDone: (w: FileWatcher | null) => void }) {
  const [w, setW] = createSignal<FileWatcher>({ ...props.watcher, outputFilters: [...props.watcher.outputFilters] });
  const [problems, setProblems] = createSignal<string[]>([]);
  const inputs: Partial<Record<TextField, HTMLInputElement>> = {};
  const set = (patch: Partial<FileWatcher>) => setW((x) => ({ ...x, ...patch }));
  const scopes = () => scopeEntries().entries.map((e) => e.name);

  const insertMacro = (field: TextField, e: MouseEvent) => {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    showContextMenu(
      MACROS.map((m) =>
        item(m, () => {
          const el = inputs[field];
          const value = w()[field];
          const at = el?.selectionStart ?? value.length;
          const end = el?.selectionEnd ?? at;
          set({ [field]: value.slice(0, at) + m + value.slice(end) } as Partial<FileWatcher>);
        }),
      ),
      r.left,
      r.bottom + 2,
    );
  };

  const ok = async () => {
    const out = { ...w(), name: w().name.trim(), outputFilters: w().outputFilters.map((f) => f.trim()).filter(Boolean) };
    const found = await fileWatchersApi.validate(out, props.others).catch((err) => [String(err)]);
    setProblems(found);
    if (!found.length) props.onDone(out);
  };

  const Text = (p: { field: TextField; label: string; placeholder?: string; macros?: boolean }) => (
    <div class="et-row">
      <label class="et-label" for={`fw-${p.field}`}>
        {p.label}
      </label>
      <input
        id={`fw-${p.field}`}
        ref={(el) => (inputs[p.field] = el)}
        class="set-input et-grow et-mono"
        value={w()[p.field]}
        placeholder={p.placeholder}
        onInput={(e) => set({ [p.field]: e.currentTarget.value } as Partial<FileWatcher>)}
      />
      <Show when={p.macros !== false}>
        <button class="btn-secondary et-macro-btn" title={tr("langs.webTooling.insertMacro")} onClick={(e) => insertMacro(p.field, e)}>
          <Plus size={13} /> {tr("langs.webTooling.macro")}
        </button>
      </Show>
    </div>
  );

  const Check = (p: { key: "autoSave" | "onExternal" | "trackOnlyRoot"; label: string }) => (
    <label class="fw-checkbox">
      <input type="checkbox" checked={w()[p.key]} onChange={(e) => set({ [p.key]: e.currentTarget.checked } as Partial<FileWatcher>)} />
      {p.label}
    </label>
  );

  return (
    <div class="popup-backdrop modal et-modal">
      <div
        class="popup et-dialog fw-dialog"
        role="dialog"
        aria-label={props.creating ? tr("langs.webTooling.newWatcher") : tr("langs.webTooling.editWatcher")}
        onKeyDown={(e) => {
          if (e.key === "Escape") {
            e.preventDefault();
            e.stopPropagation();
            props.onDone(null);
          } else if (e.key === "Enter" && !(e.target as Element).matches("textarea, button")) {
            e.preventDefault();
            e.stopPropagation();
            void ok();
          }
        }}
      >
        <div class="et-title">{props.creating ? tr("langs.webTooling.newWatcher") : tr("langs.webTooling.editWatcher")}</div>
        <div class="et-form">
          <div class="et-row">
            <label class="et-label" for="fw-name">
              {tr("common.nameLabel")}
            </label>
            <input id="fw-name" class="set-input et-grow" value={w().name} onInput={(e) => set({ name: e.currentTarget.value })} />
          </div>
          <div class="et-section">{tr("langs.webTooling.filesToWatch")}</div>
          <div class="et-row">
            <label class="et-label" for="fw-type">
              {tr("langs.webTooling.fileTypeLabel")}
            </label>
            <select id="fw-type" class="set-input" value={w().fileType} onChange={(e) => set({ fileType: e.currentTarget.value })}>
              <For each={WATCHER_FILE_TYPES}>{(t) => <option value={t.id}>{t.label}</option>}</For>
            </select>
            <label class="et-label et-group-label" for="fw-scope">
              {tr("common.scopeLabel")}
            </label>
            <select id="fw-scope" class="set-input et-grow" value={w().scope || "Project Files"} onChange={(e) => set({ scope: e.currentTarget.value })}>
              <For each={scopes().length ? scopes() : ["Project Files"]}>{(s) => <option value={s}>{s}</option>}</For>
            </select>
          </div>
          <div class="et-section">{tr("langs.webTooling.toolToRunOnChanges")}</div>
          <Text field="program" label={tr("langs.webTooling.programLabel")} placeholder="sass" />
          <Text field="arguments" label={tr("common.argumentsLabel")} placeholder={tr("langs.webTooling.filenameFilenamewithoutextensionCss")} />
          <Text field="outputPaths" label={tr("langs.webTooling.outputPathsToRefreshLabel")} placeholder={tr("langs.webTooling.filenamewithoutextensionCss")} />
          <Text field="workingDirectory" label={tr("langs.webTooling.workingDirectoryLabel")} placeholder={tr("langs.webTooling.filedir")} />
          <div class="et-row">
            <label class="et-label" for="fw-env">
              {tr("langs.webTooling.environmentVariablesLabel")}
            </label>
            <input
              id="fw-env"
              class="set-input et-grow et-mono"
              value={w().environment.split("\n").filter(Boolean).join(";")}
              placeholder={tr("langs.webTooling.nodeEnvProductionEllipsis")}
              onInput={(e) => set({ environment: e.currentTarget.value.split(";").map((s) => s.trim()).filter(Boolean).join("\n") })}
            />
          </div>
          <div class="et-section">{tr("langs.webTooling.advancedOptions")}</div>
          <div class="fw-options">
            <Check key="autoSave" label={tr("langs.webTooling.autoSaveEditedFilesTo")} />
            <Check key="onExternal" label={tr("langs.webTooling.triggerTheWatcherOnExternal")} />
            <Check key="trackOnlyRoot" label={tr("langs.webTooling.trackOnlyRootFiles")} />
            <div class="et-row">
              <label class="et-label" for="fw-console">
                {tr("langs.webTooling.showConsoleLabel")}
              </label>
              <select id="fw-console" class="set-input" value={w().showConsole} onChange={(e) => set({ showConsole: e.currentTarget.value as ShowConsole })}>
                <option value="always">{tr("langs.webTooling.always")}</option>
                <option value="error">{tr("langs.webTooling.onError")}</option>
                <option value="never">{tr("langs.webTooling.never")}</option>
              </select>
            </div>
          </div>
          <div class="et-row et-filters-row">
            <label class="et-label" for="fw-filters">
              {tr("langs.webTooling.outputFiltersLabel")}
            </label>
            <textarea
              id="fw-filters"
              class="set-input et-grow et-mono fw-filters"
              rows={3}
              value={w().outputFilters.join("\n")}
              placeholder={tr("langs.webTooling.filePathLineColumn")}
              onInput={(e) => set({ outputFilters: e.currentTarget.value.split("\n") })}
            />
          </div>
          <div class="set-desc fw-filter-hint">{tr("langs.webTooling.oneRegularExpressionPerLine")}</div>
        </div>
        <Show when={problems().length}>
          <div class="et-problems" role="alert">
            <For each={problems()}>{(p) => <div>{p}</div>}</For>
          </div>
        </Show>
        <div class="dialog-buttons et-buttons">
          <span class="et-spacer" />
          <button class="btn-primary" onClick={() => void ok()}>
            OK
          </button>
          <button class="btn-secondary" onClick={() => props.onDone(null)}>
            {tr("common.cancel")}
          </button>
        </div>
      </div>
    </div>
  );
}
