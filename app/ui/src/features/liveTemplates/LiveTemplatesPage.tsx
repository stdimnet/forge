/**
 * Settings | Editor | Live Templates, laid out like PHPStorm's page: the expand key, the
 * template groups with check boxes, and an editor for the selected template (abbreviation,
 * description, template text, variables and the contexts it is applicable in).
 *
 * Edits go to the Settings dialog draft (`editor.liveTemplates.custom` / `.disabled`), so
 * Apply, Cancel and Reset work as on every other page. Rust resolves the groups from the
 * draft (`live_templates_catalog`); the form keeps its own copy of the selected template so
 * typing never waits for that round trip.
 */
import { createEffect, createMemo, createResource, createSignal, For, on, Show } from "solid-js";
import { createStore, reconcile } from "solid-js/store";
import { ChevronDown, ChevronRight, Minus, Plus, RotateCcw } from "lucide-solid";
import { draftValue, setDraft, takePageArgs } from "@/features/settings/dialogState";
import { SettingRow } from "@/features/settings/SchemaPage";
import { findSpec } from "@/state/settings";
import { liveTemplatesApi, type ContextNode, type LiveTemplate, type TemplateCatalog, type TemplateGroup } from "@/ipc/liveTemplates";
import {
  CUSTOM_KEY,
  DISABLED_KEY,
  entryFor,
  EXPAND_KEY,
  identityOf,
  mergeVariables,
  PAGE_ID,
  removeEntry,
  replaceEntry,
  restoreEntry,
  templateSettings,
  toggleContext,
  type CustomEntry,
} from "./model";
import type { NewTemplateArgs } from "./actions";

const EMPTY: TemplateCatalog = { groups: [], contexts: [], functions: [] };

const FRAMEWORK_LABEL: Record<string, string> = { laravel: "Laravel", symfony: "Symfony" };

type Form = Pick<LiveTemplate, "group" | "abbreviation" | "description" | "text" | "contexts" | "variables" | "builtin" | "key">;

export function LiveTemplatesPage() {
  const custom = () => draftValue<CustomEntry[]>(CUSTOM_KEY) ?? [];
  const disabled = () => draftValue<string[]>(DISABLED_KEY) ?? [];
  const [catalog] = createResource(
    () => ({ custom: custom(), disabled: disabled() }),
    (values) => liveTemplatesApi.catalog(templateSettings(values)).catch(() => EMPTY),
    { initialValue: EMPTY },
  );
  const groups = () => catalog.latest?.groups ?? [];
  const templates = createMemo(() => groups().flatMap((g) => g.templates));

  /** Selection: a template identity (`identityOf`) or `group:<name>`. */
  const [selected, setSelected] = createSignal<string | null>(null);
  const [expanded, setExpanded] = createStore<Record<string, boolean>>({});
  const [filter, setFilter] = createSignal("");
  const [form, setForm] = createStore<{ t: Form | null }>({ t: null });
  const [showVariables, setShowVariables] = createSignal(false);
  const [showContexts, setShowContexts] = createSignal(false);
  let abbreviationInput: HTMLInputElement | undefined;

  const selectedTemplate = () => templates().find((t) => identityOf(t) === selected());
  const selectedGroup = (): string | null => {
    const s = selected();
    if (s?.startsWith("group:")) return s.slice(6);
    return selectedTemplate()?.group ?? null;
  };

  const load = (t: Form | null) => {
    setForm("t", reconcile(t ? { ...t, variables: t.variables.map((v) => ({ ...v })), contexts: [...t.contexts] } : null));
    setShowContexts(false);
  };
  const select = (t: LiveTemplate) => {
    setSelected(identityOf(t));
    load(t);
  };

  /** Write the form to the draft. */
  const save = () => {
    const t = form.t;
    if (!t) return;
    const identity = { id: `${t.group}/${t.abbreviation}`, builtin: t.builtin, key: t.key };
    const original = selectedTemplate() ?? identity;
    setDraft(CUSTOM_KEY, replaceEntry(custom(), original, entryFor(t)));
  };
  const edit = <K extends keyof Form>(key: K, value: Form[K]) => {
    setForm("t", key, value as never);
    save();
  };

  // Template text changes: refresh the variables list (names come from Rust).
  let textSeq = 0;
  const editText = async (text: string) => {
    edit("text", text);
    const seq = ++textSeq;
    const names = await liveTemplatesApi.variables(text).catch(() => null);
    if (!names || seq !== textSeq || !form.t) return;
    edit("variables", mergeVariables(names, form.t.variables));
  };

  const addTemplate = (args?: NewTemplateArgs) => {
    const group = args ? "user" : (selectedGroup() ?? "user");
    const sample = groups().find((g) => g.name === group)?.templates[0];
    const t: Form = {
      group,
      abbreviation: "",
      description: "",
      text: args?.text ?? "",
      contexts: args?.contexts ?? sample?.contexts ?? [],
      variables: [],
      key: `t${Date.now().toString(36)}`,
    };
    setDraft(CUSTOM_KEY, [...custom(), entryFor(t)]);
    setExpanded(group, true);
    setSelected(`key:${t.key}`);
    load(t);
    if (args) void editText(t.text);
    queueMicrotask(() => abbreviationInput?.focus());
  };

  const removeSelected = () => {
    const t = selectedTemplate();
    if (!t) return;
    setDraft(CUSTOM_KEY, removeEntry(custom(), t));
    setSelected(`group:${t.group}`);
    load(null);
  };

  /** Back to the shipped state: the selected built-in template, or every built-in of the group. */
  const restore = () => {
    const t = selectedTemplate();
    const group = selectedGroup();
    if (t?.builtin) {
      // The form shows the shipped template once the catalog has it (effect below).
      setDraft(CUSTOM_KEY, restoreEntry(custom(), t));
      load(null);
    } else if (group) {
      setDraft(
        CUSTOM_KEY,
        custom().filter((e) => !(e.builtin && e.builtin.startsWith(`${group}/`))),
      );
    }
  };
  const canRestore = () => {
    const t = selectedTemplate();
    if (t) return t.modified;
    const group = selectedGroup();
    return !!group && custom().some((e) => e.builtin?.startsWith(`${group}/`));
  };

  const setEnabled = (ids: string[], on: boolean) => {
    const rest = disabled().filter((d) => !ids.includes(d));
    setDraft(DISABLED_KEY, on ? rest : [...rest, ...ids]);
  };

  // A selected template without a form (restored defaults): show it once resolved.
  createEffect(
    on(templates, () => {
      const t = selectedTemplate();
      if (t && !form.t) load(t);
    }),
  );

  // Save as Live Template: a new template with the selected text.
  createEffect(() => {
    const args = takePageArgs<NewTemplateArgs>(PAGE_ID);
    if (args) addTemplate(args);
  });

  const matches = (t: LiveTemplate) => {
    const f = filter().toLowerCase();
    return !f || t.abbreviation.toLowerCase().includes(f) || t.description.toLowerCase().includes(f);
  };
  const visibleGroups = createMemo(() =>
    groups()
      .map((g) => ({ group: g, templates: g.templates.filter(matches) }))
      .filter((g) => !filter() || g.templates.length),
  );

  const expandSpec = () => findSpec(EXPAND_KEY);

  return (
    <div class="lt-page">
      <Show when={expandSpec()}>
        <div class="set-section-body lt-expand-row">
          <SettingRow spec={expandSpec()!} />
        </div>
      </Show>

      <div class="set-list lt-tree-box">
        <div class="set-list-toolbar">
          <button class="icon-btn" title="Add Live Template" onClick={() => addTemplate()}>
            <Plus size={14} />
          </button>
          <button class="icon-btn" title="Remove" disabled={!selectedTemplate()} onClick={removeSelected}>
            <Minus size={14} />
          </button>
          <button class="icon-btn" title="Restore Defaults" disabled={!canRestore()} onClick={restore}>
            <RotateCcw size={13} />
          </button>
          <input
            class="set-input lt-filter"
            placeholder="Search templates"
            spellcheck={false}
            value={filter()}
            onInput={(e) => setFilter(e.currentTarget.value)}
          />
        </div>
        <div class="lt-tree" role="tree">
          <For each={visibleGroups()}>
            {(entry) => (
              <GroupRows
                group={entry.group}
                templates={entry.templates}
                open={!!expanded[entry.group.name] || !!filter()}
                toggleOpen={() => setExpanded(entry.group.name, (v) => !v)}
                selected={selected()}
                onSelectGroup={() => {
                  setSelected(`group:${entry.group.name}`);
                  load(null);
                }}
                onSelect={select}
                setEnabled={setEnabled}
              />
            )}
          </For>
        </div>
      </div>

      <Show
        when={form.t}
        fallback={<div class="lt-hint">{selected()?.startsWith("group:") ? "Select a template to edit it." : "Select a template or add one with +."}</div>}
      >
        {(t) => (
          <div class="lt-editor">
            <div class="lt-editor-fields">
              <label class="set-label" for="lt-abbr">
                Abbreviation:
              </label>
              <input
                id="lt-abbr"
                ref={abbreviationInput}
                class="set-input lt-abbr"
                classList={{ invalid: !t().abbreviation.trim() }}
                spellcheck={false}
                value={t().abbreviation}
                onInput={(e) => edit("abbreviation", e.currentTarget.value.trim())}
              />
              <label class="set-label lt-desc-label" for="lt-desc">
                Description:
              </label>
              <input id="lt-desc" class="set-input lt-desc" value={t().description} onInput={(e) => edit("description", e.currentTarget.value)} />
            </div>
            <div class="lt-text-row">
              <div class="lt-text-col">
                <label class="set-label-block" for="lt-text">
                  Template text:
                </label>
                <textarea id="lt-text" class="lt-text" spellcheck={false} value={t().text} onInput={(e) => void editText(e.currentTarget.value)} />
              </div>
              <div class="lt-side">
                <button class="btn-secondary set-btn" onClick={() => setShowVariables((v) => !v)} disabled={!t().variables.length}>
                  Edit Variables
                </button>
                <div class="lt-side-hint">
                  Use <code>$NAME$</code> for variables, <code>$END$</code> for the caret and <code>$SELECTION$</code> for the surrounded text.
                </div>
              </div>
            </div>
            <Show when={showVariables() && t().variables.length}>
              <VariablesTable
                variables={t().variables}
                functions={catalog.latest?.functions ?? []}
                onChange={(variables) => edit("variables", variables)}
              />
            </Show>
            <div class="lt-applicable">
              <Show when={t().contexts.length} fallback={<span class="lt-warning">No applicable contexts.</span>}>
                <span>
                  Applicable in {selectedTemplate()?.applicable || t().contexts.join(", ")}.
                </span>
              </Show>
              <button class="set-link lt-link" onClick={() => setShowContexts((v) => !v)}>
                {t().contexts.length ? "Change" : "Define"}
              </button>
            </div>
            <Show when={showContexts()}>
              <ContextTree nodes={catalog.latest?.contexts ?? []} contexts={t().contexts} onChange={(c) => edit("contexts", c)} />
            </Show>
          </div>
        )}
      </Show>
    </div>
  );
}

function GroupRows(props: {
  group: TemplateGroup;
  templates: LiveTemplate[];
  open: boolean;
  toggleOpen: () => void;
  selected: string | null;
  onSelectGroup: () => void;
  onSelect: (t: LiveTemplate) => void;
  setEnabled: (ids: string[], on: boolean) => void;
}) {
  const ids = () => props.group.templates.map((t) => t.id);
  const enabledCount = () => props.group.templates.filter((t) => t.enabled).length;
  return (
    <>
      <div
        class="lt-row lt-group-row"
        role="treeitem"
        aria-expanded={props.open}
        data-group={props.group.name}
        classList={{ selected: props.selected === `group:${props.group.name}`, inactive: !props.group.active }}
        onClick={props.onSelectGroup}
        onDblClick={props.toggleOpen}
      >
        <span
          class="lt-chevron"
          onClick={(e) => {
            e.stopPropagation();
            props.toggleOpen();
          }}
        >
          {props.open ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
        </span>
        <input
          type="checkbox"
          checked={enabledCount() === props.group.templates.length && enabledCount() > 0}
          ref={(el) => createEffect(() => (el.indeterminate = enabledCount() > 0 && enabledCount() < props.group.templates.length))}
          onClick={(e) => e.stopPropagation()}
          onChange={(e) => props.setEnabled(ids(), e.currentTarget.checked)}
        />
        <span class="lt-group-name">{props.group.name}</span>
        <Show when={props.group.framework}>
          <span class="lt-row-detail">{FRAMEWORK_LABEL[props.group.framework!] ?? props.group.framework} projects</span>
        </Show>
      </div>
      <Show when={props.open}>
        <For each={props.templates}>
          {(t) => (
            <div
              class="lt-row lt-template-row"
              role="treeitem"
              data-template={t.id}
              classList={{ selected: props.selected === identityOf(t), modified: t.modified }}
              onClick={() => props.onSelect(t)}
            >
              <input
                type="checkbox"
                checked={t.enabled}
                onClick={(e) => e.stopPropagation()}
                onChange={(e) => props.setEnabled([t.id], e.currentTarget.checked)}
              />
              <span class="lt-abbr-name">{t.abbreviation || "<no abbreviation>"}</span>
              <Show when={t.description}>
                <span class="lt-row-detail">({t.description})</span>
              </Show>
            </div>
          )}
        </For>
      </Show>
    </>
  );
}

function VariablesTable(props: {
  variables: LiveTemplate["variables"];
  functions: [string, string][];
  onChange: (v: LiveTemplate["variables"]) => void;
}) {
  const set = (i: number, patch: Partial<LiveTemplate["variables"][number]>) =>
    props.onChange(props.variables.map((v, j) => (j === i ? { ...v, ...patch } : v)));
  return (
    <div class="lt-vars">
      <table class="set-table lt-vars-table">
        <thead>
          <tr>
            <th>Name</th>
            <th>Expression</th>
            <th>Default value</th>
            <th>Skip if defined</th>
          </tr>
        </thead>
        <tbody>
          <For each={props.variables}>
            {(v, i) => (
              <tr data-variable={v.name}>
                <td class="set-table-mono">{v.name}</td>
                <td>
                  <input
                    class="set-input lt-var-input"
                    list="lt-functions"
                    spellcheck={false}
                    value={v.expression}
                    onInput={(e) => set(i(), { expression: e.currentTarget.value })}
                  />
                </td>
                <td>
                  <input class="set-input lt-var-input" spellcheck={false} value={v.defaultValue} onInput={(e) => set(i(), { defaultValue: e.currentTarget.value })} />
                </td>
                <td class="lt-var-skip">
                  <input type="checkbox" checked={!v.alwaysStopAt} onChange={(e) => set(i(), { alwaysStopAt: !e.currentTarget.checked })} />
                </td>
              </tr>
            )}
          </For>
        </tbody>
      </table>
      <datalist id="lt-functions">
        <For each={props.functions}>{([sig, desc]) => <option value={sig}>{desc}</option>}</For>
      </datalist>
    </div>
  );
}

function ContextTree(props: { nodes: ContextNode[]; contexts: string[]; onChange: (c: string[]) => void }) {
  const has = (id: string) => props.contexts.includes(id);
  return (
    <div class="lt-contexts">
      <For each={props.nodes}>
        {(n) => (
          <div class="lt-context">
            <label class="set-check">
              <input type="checkbox" checked={has(n.id)} onChange={(e) => props.onChange(toggleContext(props.contexts, n.id, null, e.currentTarget.checked))} />
              <span>{n.label}</span>
            </label>
            <Show when={n.children?.length}>
              <div class="lt-context-children">
                <For each={n.children}>
                  {(c) => {
                    const implied = () => has(n.id) && !/_(COMMENT|STRING)$/.test(c.id);
                    return (
                      <label class="set-check">
                        <input
                          type="checkbox"
                          checked={has(c.id) || implied()}
                          onChange={(e) =>
                            props.onChange(
                              toggleContext(props.contexts, c.id, { id: n.id, children: n.children!.map((x) => x.id) }, e.currentTarget.checked),
                            )
                          }
                        />
                        <span>{c.label}</span>
                      </label>
                    );
                  }}
                </For>
              </div>
            </Show>
          </div>
        )}
      </For>
    </div>
  );
}
