/**
 * PHPStorm's Structural Search / Structural Replace dialog: Existing Templates on the left,
 * the search template (and the replacement template) in code editors where `$variables$`
 * are highlighted, the Filters of the selected variable (Count, Text, Type) on the right,
 * and Match case, File type and Scope below. The template is checked by Rust while it is
 * typed; "Find" / "Replace" show the matches in the Find tool window.
 */
import { createEffect, createMemo, createResource, createSignal, For, on, onCleanup, onMount, Show } from "solid-js";
import { EditorState, type Extension } from "@codemirror/state";
import { Decoration, EditorView, MatchDecorator, ViewPlugin, placeholder, type DecorationSet, type ViewUpdate } from "@codemirror/view";
import { php } from "@codemirror/lang-php";
import { javascript } from "@codemirror/lang-javascript";
import { ChevronDown, ChevronRight, PanelLeft } from "lucide-solid";
import { forgeEditorTheme, forgeHighlight } from "@/editor/forgeTheme";
import { ssrApi, type SsrConstraint, type SsrFileType, type SsrSavedTemplate, type SsrTemplateCheck } from "@/ipc/ssr";
import { ScopeSelect } from "@/features/scopes/ScopeSelect";
import { closeSsrDialog, FILE_TYPES, runSsr, setSsrForm, ssrForm, useTemplate, type SsrMode } from "./state";

const variableMarks = new MatchDecorator({
  regexp: /\$[A-Za-z_][A-Za-z0-9_]*\$/g,
  decoration: Decoration.mark({ class: "ssr-var" }),
});

const highlightVariables = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet;
    constructor(view: EditorView) {
      this.decorations = variableMarks.createDeco(view);
    }
    update(u: ViewUpdate) {
      this.decorations = variableMarks.updateDeco(u, this.decorations);
    }
  },
  { decorations: (v) => v.decorations },
);

function language(type: SsrFileType): Extension {
  if (type === "php") return php({ plain: true });
  return javascript({ typescript: type === "typeScript" });
}

/** A small code editor for a template. */
function TemplateEditor(props: { value: string; fileType: SsrFileType; onChange: (v: string) => void; label: string; autofocus?: boolean }) {
  let host!: HTMLDivElement;
  let view: EditorView | undefined;
  const build = () => {
    view?.destroy();
    view = new EditorView({
      parent: host,
      state: EditorState.create({
        doc: props.value,
        extensions: [
          forgeEditorTheme,
          forgeHighlight,
          language(props.fileType),
          highlightVariables,
          EditorView.lineWrapping,
          placeholder("Code with $variables$, e.g. $Instance$->$Method$($Args$)"),
          EditorView.contentAttributes.of({ "aria-label": props.label }),
          EditorView.updateListener.of((u) => {
            if (u.docChanged) props.onChange(u.state.doc.toString());
          }),
        ],
      }),
    });
  };
  onMount(() => {
    build();
    if (props.autofocus) queueMicrotask(() => view?.focus());
  });
  // A template from the library or another file type replaces the editor's content.
  createEffect(
    on([() => props.fileType], () => build(), { defer: true }),
  );
  createEffect(
    on(
      () => props.value,
      (v) => {
        if (view && view.state.doc.toString() !== v) view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: v } });
      },
      { defer: true },
    ),
  );
  onCleanup(() => view?.destroy());
  return <div class="ssr-editor" ref={host} />;
}

/** The library: built-in templates by file type and category. */
function TemplateLibrary(props: { onPick: (t: SsrSavedTemplate) => void }) {
  const [templates] = createResource(() => ssrApi.templates().catch(() => [] as SsrSavedTemplate[]));
  const [collapsed, setCollapsed] = createSignal<Set<string>>(new Set());
  const groups = createMemo(() => {
    const out: { key: string; label: string; items: SsrSavedTemplate[] }[] = [];
    for (const t of templates() ?? []) {
      const type = FILE_TYPES.find((f) => f.value === t.fileType)?.label ?? t.fileType;
      const key = `${type}/${t.category}`;
      let g = out.find((x) => x.key === key);
      if (!g) out.push((g = { key, label: `${type} › ${t.category}`, items: [] }));
      g.items.push(t);
    }
    return out;
  });
  const toggle = (key: string) =>
    setCollapsed((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  return (
    <div class="ssr-library" role="tree" aria-label="Existing Templates">
      <div class="ssr-library-title">Existing Templates</div>
      <For each={groups()}>
        {(g) => (
          <>
            <div class="ssr-library-group" onClick={() => toggle(g.key)}>
              {collapsed().has(g.key) ? <ChevronRight size={13} /> : <ChevronDown size={13} />}
              {g.label}
            </div>
            <Show when={!collapsed().has(g.key)}>
              <For each={g.items}>
                {(t) => (
                  <div class="ssr-library-item" data-template={t.name} title={t.template} onClick={() => props.onPick(t)}>
                    {t.name}
                  </div>
                )}
              </For>
            </Show>
          </>
        )}
      </For>
    </div>
  );
}

/** The Filters panel of one variable. */
function Filters(props: { name: string }) {
  const c = (): SsrConstraint => ssrForm.constraints[props.name] ?? { name: props.name };
  const set = (patch: Partial<SsrConstraint>) => setSsrForm("constraints", props.name, { ...c(), ...patch, name: props.name });
  const min = () => c().minCount ?? 1;
  const max = () => (c().maxCount === undefined ? 1 : c().maxCount);
  return (
    <div class="ssr-filters">
      <div class="ssr-filter-title">
        Filters for <span class="ssr-var">${props.name}$</span>
      </div>
      <div class="ssr-filter-row">
        <span class="ssr-filter-label">Count:</span>
        <input class="ssr-num" type="number" min="0" value={min()} aria-label="Minimum count" onInput={(e) => set({ minCount: Math.max(0, Number(e.currentTarget.value) || 0) })} />
        <span>to</span>
        <Show
          when={max() !== null}
          fallback={<span class="ssr-infinite">∞</span>}
        >
          <input class="ssr-num" type="number" min="0" value={max() ?? 1} aria-label="Maximum count" onInput={(e) => set({ maxCount: Math.max(0, Number(e.currentTarget.value) || 0) })} />
        </Show>
        <label class="form-label check">
          <input type="checkbox" checked={max() === null} onChange={(e) => set({ maxCount: e.currentTarget.checked ? null : Math.max(1, min()) })} />
          Unlimited
        </label>
      </div>
      <div class="ssr-filter-row">
        <span class="ssr-filter-label">Text:</span>
        <input class="ssr-text" value={c().text ?? ""} placeholder="Regular expression" spellcheck={false} onInput={(e) => set({ text: e.currentTarget.value })} />
        <label class="form-label check">
          <input type="checkbox" checked={!!c().invertText} onChange={(e) => set({ invertText: e.currentTarget.checked })} />
          Invert
        </label>
      </div>
      <div class="ssr-filter-row">
        <span class="ssr-filter-label">Type:</span>
        <input class="ssr-text" value={c().type ?? ""} placeholder="Order|string" spellcheck={false} onInput={(e) => set({ type: e.currentTarget.value })} />
        <label class="form-label check">
          <input type="checkbox" checked={!!c().invertType} onChange={(e) => set({ invertType: e.currentTarget.checked })} />
          Invert
        </label>
      </div>
      <div class="ssr-filter-hint muted">Type works where the code shows it: literals, new expressions, typed parameters, $this.</div>
    </div>
  );
}

export function SsrDialog(props: { mode: SsrMode }) {
  const replace = () => props.mode === "replace";
  const [showLibrary, setShowLibrary] = createSignal(true);
  const [check, setCheck] = createSignal<SsrTemplateCheck>({ variables: [] });
  const [selectedVar, setSelectedVar] = createSignal<string | null>(null);
  const [busy, setBusy] = createSignal(false);
  let timer: ReturnType<typeof setTimeout> | undefined;

  // Rust checks the template (variables, syntax) shortly after each edit.
  createEffect(
    on([() => ssrForm.template, () => ssrForm.replacement, () => ssrForm.fileType, () => JSON.stringify(ssrForm.constraints)], () => {
      clearTimeout(timer);
      timer = setTimeout(() => {
        const request = {
          template: ssrForm.template,
          fileType: ssrForm.fileType,
          constraints: Object.values(ssrForm.constraints),
          replacement: replace() ? ssrForm.replacement : undefined,
        };
        void ssrApi.check(request).then(
          (c) => {
            setCheck(c);
            if (!c.variables.includes(selectedVar() ?? "")) setSelectedVar(c.variables[0] ?? null);
          },
          () => undefined,
        );
      }, 150);
    }),
  );
  onCleanup(() => clearTimeout(timer));

  const run = async () => {
    if (busy() || !ssrForm.template.trim() || check().error) return;
    setBusy(true);
    try {
      if (await runSsr(props.mode)) closeSsrDialog();
    } finally {
      setBusy(false);
    }
  };

  return (
    <div class="popup-backdrop modal" onMouseDown={(e) => e.target === e.currentTarget && closeSsrDialog()}>
      <div
        class="dialog ssr-dialog"
        role="dialog"
        aria-label={replace() ? "Structural Replace" : "Structural Search"}
        onKeyDown={(e) => {
          if (e.key === "Escape") closeSsrDialog();
          else if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) void run();
          else return;
          e.preventDefault();
          e.stopPropagation();
        }}
      >
        <div class="dialog-title ssr-title">
          <button
            class="icon-btn"
            classList={{ on: showLibrary() }}
            title="Show Templates"
            aria-label="Show Templates"
            aria-pressed={showLibrary()}
            onClick={() => setShowLibrary(!showLibrary())}
          >
            <PanelLeft size={15} />
          </button>
          {replace() ? "Structural Replace" : "Structural Search"}
        </div>
        <div class="ssr-body">
          <Show when={showLibrary()}>
            <TemplateLibrary onPick={useTemplate} />
          </Show>
          <div class="ssr-main">
            <div class="ssr-label">Search template:</div>
            <TemplateEditor
              label="Search template"
              value={ssrForm.template}
              fileType={ssrForm.fileType}
              autofocus
              onChange={(v) => setSsrForm("template", v)}
            />
            <Show when={check().error}>
              <div class="ssr-error">{check().error}</div>
            </Show>
            <Show when={replace()}>
              <div class="ssr-label">Replace template:</div>
              <TemplateEditor
                label="Replace template"
                value={ssrForm.replacement}
                fileType={ssrForm.fileType}
                onChange={(v) => setSsrForm("replacement", v)}
              />
              <Show when={check().unknownInReplacement?.length}>
                <div class="ssr-error">
                  Not in the search template: {check().unknownInReplacement!.map((v) => `$${v}$`).join(", ")}
                </div>
              </Show>
            </Show>
            <div class="ssr-options">
              <label class="form-label check">
                <input type="checkbox" checked={ssrForm.matchCase} onChange={(e) => setSsrForm("matchCase", e.currentTarget.checked)} />
                Match case
              </label>
              <label class="ssr-option">
                File type:
                <select value={ssrForm.fileType} aria-label="File type" onChange={(e) => setSsrForm("fileType", e.currentTarget.value as SsrFileType)}>
                  <For each={FILE_TYPES}>{(t) => <option value={t.value}>{t.label}</option>}</For>
                </select>
              </label>
              <label class="ssr-option">
                Scope:
                <ScopeSelect value={ssrForm.scopeName} onChange={(n) => setSsrForm("scopeName", n)} />
              </label>
            </div>
          </div>
          <div class="ssr-side">
            <div class="ssr-vars">
              <For each={check().variables} fallback={<div class="muted ssr-novars">No variables. Add $name$ to the template.</div>}>
                {(v) => (
                  <button class="ssr-var-chip" classList={{ selected: selectedVar() === v }} onClick={() => setSelectedVar(v)}>
                    ${v}$
                  </button>
                )}
              </For>
            </div>
            <Show when={selectedVar()} keyed>
              {(v) => <Filters name={v} />}
            </Show>
          </div>
        </div>
        <div class="dialog-buttons">
          <span class="dialog-checkbox muted ssr-hint">Ctrl+Enter to {replace() ? "replace" : "find"}</span>
          <button class="btn-secondary" onClick={closeSsrDialog}>
            Cancel
          </button>
          <button class="btn-primary" disabled={busy() || !ssrForm.template.trim() || !!check().error} onClick={() => void run()}>
            {replace() ? "Replace" : "Find"}
          </button>
        </div>
      </div>
    </div>
  );
}
