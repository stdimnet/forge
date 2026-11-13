/**
 * Settings | Editor | File Types, like PHPStorm: the recognized file types with their file
 * name patterns (add, remove, edit; a pattern moves from the type that had it), custom file
 * types with their own highlighting, and the ignored files and folders. The rows come from
 * Rust (`filetypes_list`) with the page's draft applied, so Apply / Cancel work as usual.
 */
import { createMemo, createResource, createSignal, For, Show } from "solid-js";
import { FileType, Minus, Pencil, Plus } from "lucide-solid";
import { filetypesApi, type FileTypeRow } from "@/ipc/filetypes";
import type { SettingObject } from "@/ipc/settings";
import { fileTypeConfig } from "@/editor/fileTypes";
import { draftValue, setDraft } from "@/features/settings/dialogState";
import { CustomFileTypeForm } from "./CustomFileTypeForm";
import { addPattern, joinIgnored, ownerOf, removeCustomType, removePattern, splitIgnored } from "./model";
import { t } from "@/i18n";

export const ASSOCIATIONS_KEY = "fileTypes.associations";
export const CUSTOM_KEY = "fileTypes.custom";
export const IGNORED_KEY = "fileTypes.ignoredFiles";

/** A one-line text form (add or edit a pattern / an ignored name). */
function LineForm(props: {
  label: string;
  initial: string;
  /** Text under the field for the current value. */
  hint?: (value: string) => string | undefined;
  onSave: (v: string) => void;
  onCancel: () => void;
}) {
  const [value, setValue] = createSignal(props.initial);
  const save = () => value().trim() && props.onSave(value().trim());
  const hint = () => props.hint?.(value().trim());
  return (
    <div class="ft-line-form">
      <label class="set-label">{props.label}</label>
      <input
        class="set-input"
        value={value()}
        spellcheck={false}
        ref={(el) => queueMicrotask(() => el.focus())}
        onInput={(e) => setValue(e.currentTarget.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter") save();
          else if (e.key === "Escape") props.onCancel();
          else return;
          e.preventDefault();
          e.stopPropagation();
        }}
      />
      <button class="btn-primary set-btn" disabled={!value().trim()} onClick={save}>
        OK
      </button>
      <button class="btn-secondary set-btn" onClick={() => props.onCancel()}>
        {t("common.cancel")}
      </button>
      <Show when={hint()}>
        <div class="ft-hint">{hint()}</div>
      </Show>
    </div>
  );
}

export function FileTypesPage() {
  const associations = () => draftValue<SettingObject[]>(ASSOCIATIONS_KEY) ?? [];
  const custom = () => draftValue<SettingObject[]>(CUSTOM_KEY) ?? [];
  const ignored = () => splitIgnored(draftValue<string>(IGNORED_KEY) ?? "");

  const request = () => ({ files: [], ...fileTypeConfig(), associations: associations(), customTypes: custom() });
  const [rows] = createResource(
    () => JSON.stringify(request()),
    (json) => filetypesApi.list(JSON.parse(json)).catch(() => [] as FileTypeRow[]),
    { initialValue: [] },
  );

  const [filter, setFilter] = createSignal("");
  const [selectedId, setSelectedId] = createSignal("php");
  const [pattern, setPattern] = createSignal<string | null>(null);
  const [patternForm, setPatternForm] = createSignal<{ old: string | null } | null>(null);
  const [customForm, setCustomForm] = createSignal<{ entry: SettingObject | null } | null>(null);
  const [ignoredSel, setIgnoredSel] = createSignal<number | null>(null);
  const [ignoredForm, setIgnoredForm] = createSignal<{ index: number | null } | null>(null);

  const visible = createMemo(() => {
    const q = filter().trim().toLowerCase();
    return rows().filter((r) => !q || r.name.toLowerCase().includes(q) || r.patterns.some((p) => p.toLowerCase().includes(q)));
  });
  const selected = () => rows().find((r) => r.id === selectedId());

  const setAssociations = (next: SettingObject[]) => setDraft(ASSOCIATIONS_KEY, next);

  const savePattern = (value: string) => {
    const row = selected();
    const form = patternForm();
    if (!row || !form) return;
    let next = associations();
    if (form.old && form.old !== value) next = removePattern(next, row, form.old);
    next = addPattern(next, row, value);
    setAssociations(next);
    setPattern(value);
    setPatternForm(null);
  };
  const deletePattern = () => {
    const row = selected();
    const p = pattern();
    if (!row || !p) return;
    setAssociations(removePattern(associations(), row, p));
    setPattern(null);
  };
  const reassignHint = (value: string) => {
    const owner = ownerOf(rows(), value, selectedId());
    return owner ? `This pattern is registered by ${owner.name}; it will be reassigned to ${selected()?.name}.` : undefined;
  };

  const saveCustom = (entry: SettingObject) => {
    const others = custom().filter((c) => c.id !== entry.id);
    setDraft(CUSTOM_KEY, [...others, entry]);
    setSelectedId(entry.id);
    setCustomForm(null);
  };
  const deleteCustom = () => {
    const row = selected();
    if (!row?.custom) return;
    const next = removeCustomType(custom(), associations(), row.id);
    setDraft(CUSTOM_KEY, next.custom);
    setAssociations(next.entries);
    setSelectedId("php");
  };
  const editCustom = () => {
    const row = selected();
    if (!row?.custom) return;
    setCustomForm({ entry: custom().find((c) => c.id === row.id) ?? { id: row.id, name: row.name } });
  };

  const saveIgnored = (value: string) => {
    const form = ignoredForm();
    if (!form) return;
    const items = [...ignored()];
    if (form.index === null) items.push(value);
    else items[form.index] = value;
    setDraft(IGNORED_KEY, joinIgnored(items));
    setIgnoredForm(null);
  };
  const deleteIgnored = () => {
    const i = ignoredSel();
    if (i === null) return;
    setDraft(IGNORED_KEY, joinIgnored(ignored().filter((_, j) => j !== i)));
    setIgnoredSel(null);
  };

  return (
    <div class="set-page ft-page">
      <div class="ft-columns">
        <div class="ft-column">
          <div class="ft-caption">{t("tools.fileTypes.recognizedFileTypes")}</div>
          <div class="set-list ft-list">
            <div class="set-list-toolbar">
              <button class="icon-btn" title={t("tools.fileTypes.addCustomFileType")} data-testid="ft-add-type" onClick={() => setCustomForm({ entry: null })}>
                <Plus size={14} />
              </button>
              <button class="icon-btn" title={t("common.remove")} disabled={!selected()?.custom} onClick={deleteCustom}>
                <Minus size={14} />
              </button>
              <button class="icon-btn" title={t("common.edit")} disabled={!selected()?.custom} onClick={editCustom}>
                <Pencil size={13} />
              </button>
              <input class="set-input ft-filter" placeholder={t("tools.fileTypes.filter")} value={filter()} onInput={(e) => setFilter(e.currentTarget.value)} />
            </div>
            <div class="set-list-items ft-items" data-testid="ft-types">
              <For each={visible()}>
                {(row) => (
                  <button
                    class="set-list-item ft-type"
                    classList={{ selected: row.id === selectedId(), custom: row.custom }}
                    data-type={row.id}
                    onClick={() => {
                      setSelectedId(row.id);
                      setPattern(null);
                      setPatternForm(null);
                    }}
                    onDblClick={editCustom}
                  >
                    <FileType size={13} class="ft-icon" />
                    <span>{row.name}</span>
                    <Show when={row.description}>
                      <span class="ft-muted">{row.description}</span>
                    </Show>
                  </button>
                )}
              </For>
            </div>
          </div>
        </div>
        <div class="ft-column">
          <div class="ft-caption">{t("tools.fileTypes.fileNamePatterns", { value: selected() ? ` (${selected()!.name})` : "" })}</div>
          <div class="set-list ft-list">
            <div class="set-list-toolbar">
              <button class="icon-btn" title={t("tools.fileTypes.addPattern")} data-testid="ft-add-pattern" disabled={!selected()} onClick={() => setPatternForm({ old: null })}>
                <Plus size={14} />
              </button>
              <button class="icon-btn" title={t("common.remove")} disabled={!pattern()} onClick={deletePattern}>
                <Minus size={14} />
              </button>
              <button class="icon-btn" title={t("common.edit")} disabled={!pattern()} onClick={() => setPatternForm({ old: pattern() })}>
                <Pencil size={13} />
              </button>
            </div>
            <div class="set-list-items ft-items" data-testid="ft-patterns">
              <For each={selected()?.patterns ?? []} fallback={<div class="set-list-empty">{t("tools.fileTypes.noPatterns")}</div>}>
                {(p) => (
                  <button
                    class="set-list-item"
                    classList={{ selected: p === pattern(), "ft-added": selected()!.added.includes(p) }}
                    data-pattern={p}
                    onClick={() => setPattern(p)}
                    onDblClick={() => setPatternForm({ old: p })}
                  >
                    {p}
                  </button>
                )}
              </For>
              <For each={selected()?.removed ?? []}>
                {(p) => (
                  <span class="set-list-item ft-removed" title={t("tools.fileTypes.removedAddItAgainTo")}>
                    {p}
                  </span>
                )}
              </For>
            </div>
          </div>
          <Show when={patternForm()}>
            {(form) => (
              <LineForm
                label={form().old ? t("tools.fileTypes.editPatternLabel") : t("tools.fileTypes.addPatternLabel")}
                initial={form().old ?? "*."}
                hint={(v) => reassignHint(v) ?? "Wildcards: * matches any characters, e.g. *.module or Procfile*"}
                onSave={savePattern}
                onCancel={() => setPatternForm(null)}
              />
            )}
          </Show>
          <Show when={selected()?.syntax}>
            <div class="ft-muted ft-syntax-note">{t("tools.fileTypes.customFileTypeDoubleClick")}</div>
          </Show>
        </div>
      </div>

      <div class="ft-caption ft-ignored-caption">{t("tools.fileTypes.ignoredFilesAndFolders")}</div>
      <div class="set-list ft-ignored">
        <div class="set-list-toolbar">
          <button class="icon-btn" title={t("common.add")} onClick={() => setIgnoredForm({ index: null })}>
            <Plus size={14} />
          </button>
          <button class="icon-btn" title={t("common.remove")} disabled={ignoredSel() === null} onClick={deleteIgnored}>
            <Minus size={14} />
          </button>
          <button class="icon-btn" title={t("common.edit")} disabled={ignoredSel() === null} onClick={() => setIgnoredForm({ index: ignoredSel() })}>
            <Pencil size={13} />
          </button>
        </div>
        <div class="ft-ignored-items" data-testid="ft-ignored">
          <For each={ignored()}>
            {(name, i) => (
              <button class="set-list-item" classList={{ selected: ignoredSel() === i() }} onClick={() => setIgnoredSel(i())} onDblClick={() => setIgnoredForm({ index: i() })}>
                {name}
              </button>
            )}
          </For>
        </div>
      </div>
      <Show when={ignoredForm()}>
        {(form) => (
          <LineForm
            label={t("tools.fileTypes.ignoredNameLabel")}
            initial={form().index === null ? "" : ignored()[form().index!]}
            hint={() => "Files and folders with this name are hidden from the Project view (* and ? wildcards)."}
            onSave={saveIgnored}
            onCancel={() => setIgnoredForm(null)}
          />
        )}
      </Show>

      <Show when={customForm()}>
        {(form) => (
          <CustomFileTypeForm
            entry={form().entry}
            existingIds={rows().map((r) => r.id)}
            names={rows().filter((r) => r.id !== form().entry?.id).map((r) => r.name)}
            onSave={saveCustom}
            onCancel={() => setCustomForm(null)}
          />
        )}
      </Show>
    </div>
  );
}
