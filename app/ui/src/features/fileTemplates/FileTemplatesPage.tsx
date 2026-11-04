/**
 * Settings | Editor | File and Code Templates, like PHPStorm: Files / Includes / Code tabs,
 * the template list with Create / Remove / Copy / Reset to Default, and the name, extension
 * and text of the selected template with a description of its variables.
 *
 * Edits are a draft of the `fileTemplates.custom` setting (applied with OK / Apply like every
 * other setting). The list, the "modified" state and syntax errors come from Rust
 * (`templates_list` with the draft).
 */
import { createMemo, createResource, createSignal, For, Show } from "solid-js";
import { Copy, Minus, Plus, RotateCcw } from "lucide-solid";
import { templatesApi, type CustomTemplate, type TemplateCatalog, type TemplateInfo, type TemplateKind } from "@/ipc/templates";
import type { SettingValue } from "@/ipc/settings";
import { FileIcon } from "@/shell/FileIcon";
import { draftValue, setDraft } from "@/features/settings/dialogState";
import { CUSTOM_KEY } from "./model";
import { t as tr } from "@/i18n";

const TABS: { kind: TemplateKind; label: string }[] = [
  { kind: "file", label: tr("tools.fileTemplates.files") },
  { kind: "include", label: tr("tools.fileTemplates.includes") },
  { kind: "code", label: tr("tools.fileTemplates.code") },
];

const EMPTY: TemplateCatalog = { templates: [], variables: [] };

/** A free name: `Unnamed`, `Unnamed (2)`, … */
export function freeName(base: string, taken: string[]): string {
  if (!taken.includes(base)) return base;
  for (let i = 2; ; i++) if (!taken.includes(`${base} (${i})`)) return `${base} (${i})`;
}

/**
 * The custom entries after changing `t`: built-ins back at their default text and extension
 * drop their entry; everything else is stored in place.
 */
export function withChange(
  entries: CustomTemplate[],
  t: TemplateInfo,
  patch: { name?: string; extension?: string; text?: string },
): CustomTemplate[] {
  const next: CustomTemplate = {
    kind: t.kind,
    name: patch.name ?? t.name,
    extension: patch.extension ?? t.extension,
    text: patch.text ?? t.text,
  };
  const at = entries.findIndex((c) => c.kind === t.kind && c.name === t.name);
  const unchanged = t.builtin && next.text === t.defaultText && next.extension === t.defaultExtension;
  const rest = entries.filter((_, i) => i !== at);
  if (unchanged) return rest;
  if (at < 0) return [...entries, next];
  return [...rest.slice(0, at), next, ...rest.slice(at)];
}

export function FileTemplatesPage() {
  const [tab, setTab] = createSignal<TemplateKind>("file");
  const [selected, setSelected] = createSignal<Record<TemplateKind, string | null>>({ file: null, include: null, code: null });

  const custom = () => (draftValue<SettingValue>(CUSTOM_KEY) ?? []) as unknown as CustomTemplate[];
  const write = (next: CustomTemplate[]) => setDraft(CUSTOM_KEY, next as unknown as SettingValue);

  const [view] = createResource(
    () => JSON.stringify(custom()),
    (json) => templatesApi.list(JSON.parse(json) as CustomTemplate[]).catch(() => EMPTY),
    { initialValue: EMPTY },
  );

  const list = createMemo(() =>
    view()
      .templates.filter((t) => t.kind === tab())
      .sort((a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: "base" })),
  );
  /** The selected template; while the list reloads after a rename, the previous one. */
  const current = createMemo<TemplateInfo | undefined>((prev) => {
    const name = selected()[tab()];
    return list().find((t) => t.name === name) ?? (prev && prev.kind === tab() && prev.name === name ? prev : list()[0]);
  });
  /** The draft's text and extension win over the (asynchronously reloaded) catalog. */
  const entryOf = (t: TemplateInfo) => custom().find((c) => c.kind === t.kind && c.name === t.name);
  const textOf = (t: TemplateInfo) => entryOf(t)?.text ?? t.text;
  const extensionOf = (t: TemplateInfo) => entryOf(t)?.extension || t.extension;
  const select = (name: string) => setSelected({ ...selected(), [tab()]: name });

  /** The name being typed; applied once it is unique. */
  const [nameDraft, setNameDraft] = createSignal<{ of: string; value: string } | null>(null);
  const nameValue = () => {
    const d = nameDraft();
    return d && d.of === current()?.name ? d.value : (current()?.name ?? "");
  };
  const nameProblem = () => {
    const t = current();
    const v = nameValue().trim();
    if (!t || v === t.name) return null;
    if (!v) return "Enter a name";
    if (view().templates.some((o) => o.kind === t.kind && o.name === v)) return `A template named "${v}" already exists`;
    return null;
  };
  /** Rename a user template when the field is left or Enter is pressed. */
  const rename = () => {
    const t = current();
    const v = nameValue().trim();
    if (!t || nameProblem() || v === t.name) return;
    write(withChange(custom(), { ...t, text: textOf(t), extension: extensionOf(t) }, { name: v }));
    select(v);
    setNameDraft(null);
  };

  const add = (copyOf?: TemplateInfo) => {
    const taken = view()
      .templates.filter((t) => t.kind === tab())
      .map((t) => t.name);
    const name = freeName(copyOf ? `${copyOf.name} (copy)` : "Unnamed", taken);
    const ext = copyOf?.extension ?? (tab() === "file" ? "txt" : "php");
    write([...custom(), { kind: tab(), name, extension: ext, text: copyOf?.text ?? "" }]);
    select(name);
  };
  const remove = () => {
    const t = current();
    if (!t || t.builtin) return;
    const index = list().indexOf(t);
    write(custom().filter((c) => !(c.kind === t.kind && c.name === t.name)));
    const others = list().filter((o) => o !== t);
    setSelected({ ...selected(), [tab()]: others[Math.min(index, others.length - 1)]?.name ?? null });
  };
  const reset = () => {
    const t = current();
    if (t?.modified) write(custom().filter((c) => !(c.kind === t.kind && c.name === t.name)));
  };

  return (
    <div class="ft-page">
      <div class="ft-tabs" role="tablist">
        <For each={TABS}>
          {(t) => (
            <button
              class="ft-tab"
              role="tab"
              classList={{ active: tab() === t.kind }}
              aria-selected={tab() === t.kind}
              onClick={() => setTab(t.kind)}
            >
              {t.label}
            </button>
          )}
        </For>
      </div>
      <div class="ft-body">
        <div class="ft-list set-list">
          <div class="set-list-toolbar">
            <button class="icon-btn" title={tr("tools.fileTemplates.createTemplate")} onClick={() => add()}>
              <Plus size={14} />
            </button>
            <button class="icon-btn" title={tr("tools.fileTemplates.removeTemplate")} disabled={!current() || current()!.builtin} onClick={remove}>
              <Minus size={14} />
            </button>
            <button class="icon-btn" title={tr("tools.fileTemplates.copyTemplate")} disabled={!current()} onClick={() => add(current())}>
              <Copy size={13} />
            </button>
            <button class="icon-btn" title={tr("tools.fileTemplates.resetToDefault")} disabled={!current()?.modified} onClick={reset}>
              <RotateCcw size={13} />
            </button>
          </div>
          <div class="ft-items" role="listbox" aria-label={tr("tools.fileTemplates.templates")}>
            <For each={list()} fallback={<div class="set-list-empty">{tr("tools.fileTemplates.noTemplates")}</div>}>
              {(t) => (
                <div
                  class="ft-item"
                  role="option"
                  data-template={t.name}
                  aria-selected={current() === t}
                  classList={{ selected: current() === t, modified: t.modified, custom: !t.builtin, broken: !!t.error }}
                  onClick={() => select(t.name)}
                >
                  <FileIcon name={`x.${t.extension || "txt"}`} size={14} />
                  <span>{t.name}</span>
                </div>
              )}
            </For>
          </div>
        </div>
        <Show when={current()} fallback={<div class="ft-editor-pane set-empty">{tr("tools.fileTemplates.selectATemplate")}</div>}>
          {(sel) => (
            <div class="ft-editor-pane">
              <div class="ft-fields">
                <label class="set-label" for="ft-tpl-name">
                  {tr("common.nameLabel")}
                </label>
                <input
                  id="ft-tpl-name"
                  class="set-input"
                  classList={{ invalid: !!nameProblem() }}
                  value={nameValue()}
                  disabled={sel().builtin}
                  spellcheck={false}
                  onInput={(e) => setNameDraft({ of: sel().name, value: e.currentTarget.value })}
                  onChange={rename}
                />
                <Show when={sel().kind !== "code"}>
                  <label class="set-label" for="ft-tpl-ext">
                    {tr("tools.fileTemplates.extensionLabel")}
                  </label>
                  <input
                    id="ft-tpl-ext"
                    class="set-input ft-ext"
                    value={extensionOf(sel())}
                    spellcheck={false}
                    onInput={(e) =>
                      write(withChange(custom(), { ...sel(), text: textOf(sel()) }, { extension: e.currentTarget.value.trim().replace(/^\./, "") }))
                    }
                  />
                </Show>
              </div>
              <Show when={nameProblem()}>
                <div class="set-error">{nameProblem()}</div>
              </Show>
              <textarea
                class="ft-text"
                spellcheck={false}
                aria-label={tr("tools.fileTemplates.templateText")}
                value={textOf(sel())}
                onInput={(e) => write(withChange(custom(), { ...sel(), extension: extensionOf(sel()) }, { text: e.currentTarget.value }))}
                onKeyDown={(e) => {
                  // Tab inserts indentation instead of leaving the editor.
                  if (e.key !== "Tab" || e.shiftKey) return;
                  e.preventDefault();
                  const el = e.currentTarget;
                  el.setRangeText("    ", el.selectionStart, el.selectionEnd, "end");
                  write(withChange(custom(), { ...sel(), extension: extensionOf(sel()) }, { text: el.value }));
                }}
              />
              <Show when={sel().error}>
                <div class="set-error ft-problem">{sel().error}</div>
              </Show>
              <div class="ft-description">
                <Show when={sel().description}>
                  <p>{sel().description}</p>
                </Show>
                <Show when={sel().kind !== "code"}>
                  <p>
                    {tr("tools.fileTemplates.apacheVelocityTemplateLanguageIsLabel")} <code>{"#if (${NAMESPACE}) … #end"}</code>,{" "}
                    <code>{'#set ($X = "…")'}</code> and <code>{'#parse("PHP File Header.php")'}</code>{tr("tools.fileTemplates.theFollowingPredefinedVariablesCanLabel")}
                  </p>
                  <table class="ft-vars">
                    <tbody>
                      <For each={view().variables}>
                        {(v) => (
                          <tr>
                            <td>
                              <code>${"{"}{v.name}{"}"}</code>
                            </td>
                            <td>{v.description}</td>
                          </tr>
                        )}
                      </For>
                    </tbody>
                  </table>
                  <p>{tr("tools.fileTemplates.otherVariablesAreAskedFor")}</p>
                </Show>
              </div>
            </div>
          )}
        </Show>
      </div>
    </div>
  );
}
