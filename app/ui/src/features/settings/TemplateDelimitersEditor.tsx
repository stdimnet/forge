/**
 * Settings editor of `templates.delimiters`: a table of (template language, data language)
 * pairs with their delimiters, with Add / Remove / Edit like PHPStorm's tables, and a form
 * that shows the delimiter fields of the chosen template language.
 *
 * The configurable template languages, their default delimiters and the data languages come
 * from Rust (`filetypes_template_delimiters`); the entries are checked by the setting's schema
 * in Rust (`settings_validate`), like every other setting.
 */
import { createEffect, createResource, createSignal, For, Index, on, Show } from "solid-js";
import { Minus, Pencil, Plus } from "lucide-solid";
import { filetypesApi, type ConfigurableTemplate, type DelimiterCatalog, type DelimiterField } from "@/ipc/filetypes";
import { settingsApi, type SettingObject, type SettingSpec } from "@/ipc/settings";
import { draftValue, setDraft } from "./dialogState";
import { t as tr } from "@/i18n";

/** The value of a field: the entry's own, else the template's default. */
const valueOf = (entry: SettingObject, f: DelimiterField) => entry[f.key]?.trim() || f.default;

/** `\VAR{ }   \BLOCK{ }   \#{ }`: the start and end of each tag kind. */
export function delimiterSummary(template: ConfigurableTemplate | undefined, entry: SettingObject): string {
  const fields = template?.fields ?? [];
  const out: string[] = [];
  for (let i = 0; i < fields.length; i += 2) {
    out.push(`${valueOf(entry, fields[i])} ${fields[i + 1] ? valueOf(entry, fields[i + 1]) : ""}`.trim());
  }
  return out.join("   ");
}

/** The entry with only the fields of its template that differ from the defaults. */
export function normalizeEntry(template: ConfigurableTemplate | undefined, entry: SettingObject): SettingObject {
  const out: SettingObject = { template: entry.template, language: entry.language };
  for (const f of template?.fields ?? []) {
    const v = entry[f.key]?.trim();
    if (v && v !== f.default) out[f.key] = v;
  }
  return out;
}

/** Start/end field pairs; Smarty's and Go's left/right are one pair labelled "Delimiters". */
export function pairsOf(template: ConfigurableTemplate | undefined): [DelimiterField, DelimiterField][] {
  const fields = template?.fields ?? [];
  const pairs: [DelimiterField, DelimiterField][] = [];
  for (let i = 0; i + 1 < fields.length; i += 2) {
    const [start, end] = [fields[i], fields[i + 1]];
    pairs.push(start.key === "left" ? [{ ...start, label: tr("tools.settings.delimitersStart") }, end] : [start, end]);
  }
  return pairs;
}

const EMPTY: DelimiterCatalog = { templates: [], dataLanguages: [] };

export function TemplateDelimitersEditor(props: { spec: SettingSpec }) {
  const [catalog] = createResource(() => filetypesApi.templateDelimiters().catch(() => EMPTY), { initialValue: EMPTY });
  const templateOf = (id: string | undefined) => catalog().templates.find((t) => t.id === id);
  const templateLabel = (id: string) => templateOf(id)?.label ?? id;
  const languageLabel = (id: string) => catalog().dataLanguages.find((l) => l.id === id)?.label ?? id;

  const entries = () => draftValue<SettingObject[]>(props.spec.key) ?? [];
  const [sel, setSel] = createSignal<number | null>(null);
  /** The entry in the form, and the row it edits (`null` for a new entry). */
  const [form, setForm] = createSignal<{ index: number | null; entry: SettingObject } | null>(null);
  /** The schema's verdict on the entries with the form applied, `null` when they are valid. */
  const [problem, setProblem] = createSignal<string | null>(null);

  const update = (next: SettingObject[]) => setDraft(props.spec.key, next);
  const add = () => {
    const used = (t: string, l: string) => entries().some((e) => e.template === t && e.language === l);
    const template = catalog().templates[0]?.id ?? "smarty";
    const language = catalog().dataLanguages.find((l) => !used(template, l.id))?.id ?? "html";
    setForm({ index: null, entry: { template, language } });
  };
  const edit = () => {
    const i = sel();
    if (i !== null && entries()[i]) setForm({ index: i, entry: { ...entries()[i] } });
  };
  const remove = () => {
    const i = sel();
    if (i === null) return;
    update(entries().filter((_, j) => j !== i));
    setSel(entries().length ? Math.min(i, entries().length - 1) : null);
    setForm(null);
  };

  const current = () => form()?.entry;
  const template = () => templateOf(current()?.template);
  const set = (key: string, value: string) => setForm((f) => f && { ...f, entry: { ...f.entry, [key]: value } });

  /** The entries as they would be saved with the form's entry. */
  const withForm = (): SettingObject[] | null => {
    const f = form();
    if (!f) return null;
    const entry = normalizeEntry(templateOf(f.entry.template), f.entry);
    const next = [...entries()];
    if (f.index === null) next.push(entry);
    else next[f.index] = entry;
    return next;
  };

  let seq = 0;
  createEffect(
    on(withForm, async (next) => {
      const id = ++seq;
      const verdict = next ? await settingsApi.validate(props.spec.key, next).catch((e) => String(e)) : null;
      if (id === seq) setProblem(verdict);
    }),
  );

  // The verdict shown may still be the one of the previous edit: check exactly what is saved.
  const save = async () => {
    const f = form();
    const next = withForm();
    if (!f || !next) return;
    const id = ++seq;
    const verdict = await settingsApi.validate(props.spec.key, next).catch((e) => String(e));
    // Edited again meanwhile: the newer validation decides.
    if (id !== seq) return;
    setProblem(verdict);
    if (verdict || form() !== f) return;
    update(next);
    setSel(f.index ?? next.length - 1);
    setForm(null);
  };

  return (
    <div class="set-delims">
      <div class="set-list set-table-box">
        <div class="set-list-toolbar">
          <button class="icon-btn" title={tr("common.add")} onClick={add}>
            <Plus size={14} />
          </button>
          <button class="icon-btn" title={tr("common.remove")} disabled={sel() === null} onClick={remove}>
            <Minus size={14} />
          </button>
          <button class="icon-btn" title={tr("common.edit")} disabled={sel() === null} onClick={edit}>
            <Pencil size={13} />
          </button>
        </div>
        <table class="set-table">
          <thead>
            <tr>
              <th>{tr("tools.settings.template")}</th>
              <th>{tr("common.language")}</th>
              <th>{tr("tools.settings.delimiters")}</th>
            </tr>
          </thead>
          <tbody>
            <Index each={entries()}>
              {(entry, i) => (
                <tr
                  classList={{ selected: sel() === i }}
                  data-entry={`${entry().template}+${entry().language}`}
                  onClick={() => setSel(i)}
                  onDblClick={() => {
                    setSel(i);
                    edit();
                  }}
                >
                  <td>{templateLabel(entry().template)}</td>
                  <td>{languageLabel(entry().language)}</td>
                  <td class="set-table-mono">{delimiterSummary(templateOf(entry().template), entry())}</td>
                </tr>
              )}
            </Index>
          </tbody>
        </table>
        <Show when={!entries().length}>
          <div class="set-list-empty">{tr("common.nothingToShow")}</div>
        </Show>
      </div>

      <Show when={form()}>
        <div class="set-delims-form">
          <div class="set-delims-grid">
            <label class="set-label">{tr("tools.settings.templateLabel")}</label>
            <select class="set-input set-select" value={current()!.template} onChange={(e) => set("template", e.currentTarget.value)}>
              <For each={catalog().templates}>{(t) => <option value={t.id}>{t.label}</option>}</For>
            </select>
            <label class="set-label">{tr("tools.settings.languageLabel")}</label>
            <select class="set-input set-select" value={current()!.language} onChange={(e) => set("language", e.currentTarget.value)}>
              <For each={catalog().dataLanguages}>{(l) => <option value={l.id}>{l.label}</option>}</For>
            </select>
            <For each={pairsOf(template())}>
              {([start, end]) => (
                <>
                  <label class="set-label">{start.label.replace(/ start$/, "")}:</label>
                  <div class="set-delims-pair">
                    <input
                      class="set-input set-delims-input"
                      spellcheck={false}
                      title={start.label}
                      placeholder={start.default}
                      value={current()![start.key] ?? start.default}
                      onInput={(e) => set(start.key, e.currentTarget.value)}
                    />
                    <span class="set-desc">…</span>
                    <input
                      class="set-input set-delims-input"
                      spellcheck={false}
                      title={end.label}
                      placeholder={end.default}
                      value={current()![end.key] ?? end.default}
                      onInput={(e) => set(end.key, e.currentTarget.value)}
                    />
                  </div>
                </>
              )}
            </For>
          </div>
          <div class="set-delims-actions">
            <Show when={problem()}>
              <span class="set-error">{describeProblem(problem()!, templateLabel(current()!.template), languageLabel(current()!.language))}</span>
            </Show>
            <button class="btn-primary set-btn" disabled={!!problem()} onClick={save}>
              OK
            </button>
            <button class="btn-secondary set-btn" onClick={() => setForm(null)}>
              {tr("common.cancel")}
            </button>
          </div>
        </div>
      </Show>
    </div>
  );
}

/** The schema's message in the words of the form; a duplicate pair names the languages. */
export function describeProblem(problem: string, template: string, language: string): string {
  return /duplicate entry/.test(problem) ? `${template} in ${language} already has delimiters.` : problem;
}
