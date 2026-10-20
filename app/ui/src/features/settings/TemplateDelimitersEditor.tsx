/**
 * Settings editor of `templates.delimiters`: a table of (template language, data language)
 * pairs with their delimiters, with Add / Remove / Edit like PHPStorm's tables, and a form
 * that shows the delimiter fields of the chosen template language.
 */
import { createMemo, createSignal, For, Index, Show } from "solid-js";
import { Minus, Pencil, Plus } from "lucide-solid";
import type { SettingObject, SettingSpec } from "@/ipc/settings";
import { CONFIGURABLE_TEMPLATES, resolveDelimiters, type ConfigurableTemplate } from "@/editor/templateDelimiters";
import { DATA_LANGUAGES } from "@/editor/templateData";
import { draftValue, setDraft } from "./dialogState";

const templates = Object.values(CONFIGURABLE_TEMPLATES);
const templateLabel = (id: string) => CONFIGURABLE_TEMPLATES[id]?.label ?? id;
const languageLabel = (id: string) => DATA_LANGUAGES.find((l) => l.id === id)?.label ?? id;

/** `\VAR{ }  \BLOCK{ }  \#{ }`: the start and end of each tag kind. */
export function delimiterSummary(entry: SettingObject): string {
  const template = CONFIGURABLE_TEMPLATES[entry.template];
  if (!template) return "";
  const values = resolveDelimiters(template, [entry], entry.language);
  const out: string[] = [];
  for (let i = 0; i < template.fields.length; i += 2) {
    out.push(`${values[template.fields[i].key]} ${values[template.fields[i + 1]?.key] ?? ""}`.trim());
  }
  return out.join("   ");
}

/** The entry with only the fields of its template that differ from the defaults. */
export function normalizeEntry(entry: SettingObject): SettingObject {
  const template = CONFIGURABLE_TEMPLATES[entry.template];
  const out: SettingObject = { template: entry.template, language: entry.language };
  for (const f of template?.fields ?? []) {
    const v = entry[f.key]?.trim();
    if (v && v !== f.default) out[f.key] = v;
  }
  return out;
}

/** Index of another entry with the same pair, or -1. */
export const duplicateOf = (entries: readonly SettingObject[], entry: SettingObject, self: number | null): number =>
  entries.findIndex((e, i) => i !== self && e.template === entry.template && e.language === entry.language);

export function TemplateDelimitersEditor(props: { spec: SettingSpec }) {
  const entries = () => draftValue<SettingObject[]>(props.spec.key) ?? [];
  const [sel, setSel] = createSignal<number | null>(null);
  /** The entry in the form, and the row it edits (`null` for a new entry). */
  const [form, setForm] = createSignal<{ index: number | null; entry: SettingObject } | null>(null);

  const update = (next: SettingObject[]) => setDraft(props.spec.key, next);
  const add = () => {
    const used = (t: string, l: string) => entries().some((e) => e.template === t && e.language === l);
    const language = DATA_LANGUAGES.find((l) => !used("smarty", l.id))?.id ?? "html";
    setForm({ index: null, entry: { template: "smarty", language } });
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
  const template = (): ConfigurableTemplate | undefined => CONFIGURABLE_TEMPLATES[current()?.template ?? ""];
  const set = (key: string, value: string) => setForm((f) => f && { ...f, entry: { ...f.entry, [key]: value } });
  const duplicate = createMemo(() => {
    const f = form();
    return f ? duplicateOf(entries(), f.entry, f.index) >= 0 : false;
  });
  const save = () => {
    const f = form();
    if (!f || duplicate()) return;
    const entry = normalizeEntry(f.entry);
    const next = [...entries()];
    if (f.index === null) next.push(entry);
    else next[f.index] = entry;
    update(next);
    setSel(f.index ?? next.length - 1);
    setForm(null);
  };

  return (
    <div class="set-delims">
      <div class="set-list set-table-box">
        <div class="set-list-toolbar">
          <button class="icon-btn" title="Add" onClick={add}>
            <Plus size={14} />
          </button>
          <button class="icon-btn" title="Remove" disabled={sel() === null} onClick={remove}>
            <Minus size={14} />
          </button>
          <button class="icon-btn" title="Edit" disabled={sel() === null} onClick={edit}>
            <Pencil size={13} />
          </button>
        </div>
        <table class="set-table">
          <thead>
            <tr>
              <th>Template</th>
              <th>Language</th>
              <th>Delimiters</th>
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
                  <td class="set-table-mono">{delimiterSummary(entry())}</td>
                </tr>
              )}
            </Index>
          </tbody>
        </table>
        <Show when={!entries().length}>
          <div class="set-list-empty">Nothing to show</div>
        </Show>
      </div>

      <Show when={form()}>
        <div class="set-delims-form">
          <div class="set-delims-grid">
            <label class="set-label">Template:</label>
            <select
              class="set-input set-select"
              value={current()!.template}
              onChange={(e) => set("template", e.currentTarget.value)}
            >
              <For each={templates}>{(t) => <option value={t.id}>{t.label}</option>}</For>
            </select>
            <label class="set-label">Language:</label>
            <select
              class="set-input set-select"
              value={current()!.language}
              onChange={(e) => set("language", e.currentTarget.value)}
            >
              <For each={DATA_LANGUAGES}>{(l) => <option value={l.id}>{l.label}</option>}</For>
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
            <Show when={duplicate()}>
              <span class="set-error">
                {templateLabel(current()!.template)} in {languageLabel(current()!.language)} already has delimiters.
              </span>
            </Show>
            <button class="btn-primary set-btn" disabled={duplicate()} onClick={save}>
              OK
            </button>
            <button class="btn-secondary set-btn" onClick={() => setForm(null)}>
              Cancel
            </button>
          </div>
        </div>
      </Show>
    </div>
  );
}

/** Start/end field pairs; Smarty's and Go's left/right are one pair labelled "Delimiters". */
function pairsOf(template: ConfigurableTemplate | undefined) {
  const fields = template?.fields ?? [];
  const pairs: [ConfigurableTemplate["fields"][number], ConfigurableTemplate["fields"][number]][] = [];
  for (let i = 0; i + 1 < fields.length; i += 2) {
    const [start, end] = [fields[i], fields[i + 1]];
    pairs.push(start.key === "left" ? [{ ...start, label: "Delimiters start" }, end] : [start, end]);
  }
  return pairs;
}
