/**
 * Settings | Editor | Code Style: the General page and one page per language, like PHPStorm.
 * Every page starts with the scheme box and its ⚙ menu; language pages have "Set from…",
 * PHPStorm's tabs, the options on the left and a preview on the right that Rust formats
 * with the options as they are edited.
 */
import { createEffect, createMemo, createSignal, For, Match, on, onCleanup, onMount, Show, Switch } from "solid-js";
import { Compartment, EditorState } from "@codemirror/state";
import { EditorView, lineNumbers } from "@codemirror/view";
import { Settings } from "lucide-solid";
import { isTauri } from "@/ipc";
import { codeStyleApi, type CodeStyleExportFormat, type CodeStyleLanguage, type CodeStyleOption, type CodeStyleValue } from "@/ipc/codeStyle";
import { forgeEditorTheme, forgeHighlight } from "@/editor/forgeTheme";
import { languageFor } from "@/editor/languages";
import { ensureFileType } from "@/editor/fileTypes";
import { showBalloon } from "@/keymap";
import { showContextMenu, item, separator, submenu, type MenuEntry } from "@/features/contextMenu/state";
import { askChoice, askName } from "@/features/contextMenu/dialogs";
import { findSpec } from "@/state/settings";
import { SettingRow } from "@/features/settings/SchemaPage";
import {
  applyPreset,
  codeStyle,
  copyAs,
  copyToProject,
  current,
  deleteCurrent,
  freeName,
  importScheme,
  languageSpec,
  loadSchemes,
  nameProblem,
  renameCurrent,
  resetCurrent,
  selectScheme,
  setValue,
  valueOf,
} from "./store";

// ---- Scheme box and ⚙ menu -------------------------------------------------------------

const FORMATS: { format: CodeStyleExportFormat; label: string }[] = [
  { format: "xml", label: "IntelliJ IDEA code style XML" },
  { format: "json", label: "Forge code style JSON" },
];

async function exportScheme(format: CodeStyleExportFormat): Promise<void> {
  const scheme = current();
  if (!scheme) return;
  const file = `${scheme.name}.${format}`;
  try {
    if (isTauri()) {
      const { save } = await import("@tauri-apps/plugin-dialog");
      const label = FORMATS.find((f) => f.format === format)!.label;
      const path = await save({ defaultPath: file, filters: [{ name: label, extensions: [format] }] });
      if (!path) return;
      await codeStyleApi.export(scheme.name, scheme.values, format, path);
      showBalloon(`Scheme "${scheme.name}" exported`, path);
      return;
    }
    const text = await codeStyleApi.export(scheme.name, scheme.values, format);
    const url = URL.createObjectURL(new Blob([text], { type: format === "xml" ? "application/xml" : "application/json" }));
    const a = document.createElement("a");
    a.href = url;
    a.download = file;
    a.click();
    URL.revokeObjectURL(url);
    showBalloon(`Scheme "${scheme.name}" exported`, file);
  } catch (e) {
    showBalloon("Export failed", e instanceof Error ? e.message : String(e));
  }
}

/** Browser mode: read a file the user picks. */
function pickFileText(accept: string): Promise<string | null> {
  return new Promise((resolve) => {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = accept;
    input.onchange = () => {
      const file = input.files?.[0];
      if (!file) return resolve(null);
      void file.text().then(resolve, () => resolve(null));
    };
    input.click();
  });
}

async function importFrom(format: CodeStyleExportFormat): Promise<void> {
  try {
    let source: { text?: string; path?: string };
    if (isTauri()) {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const label = FORMATS.find((f) => f.format === format)!.label;
      const path = await open({ multiple: false, filters: [{ name: label, extensions: [format] }] });
      if (typeof path !== "string") return;
      source = { path };
    } else {
      const text = await pickFileText(`.${format}`);
      if (text === null) return;
      source = { text };
    }
    const imported = await codeStyleApi.import(source);
    const target = current();
    const choice = await askChoice({
      title: "Import Scheme",
      message: `"${imported.name}" sets ${imported.applied} options. Import them into:`,
      choices: [
        { id: "current", label: `Current Scheme (${target?.name ?? ""})` },
        { id: "new", label: "New Scheme" },
      ],
    });
    if (!choice) return;
    importScheme(imported, choice === "current");
    showBalloon(`Scheme "${imported.name}" imported`, "Apply the settings to keep it.");
  } catch (e) {
    showBalloon("Import failed", e instanceof Error ? e.message : String(e));
  }
}

async function ask(title: string, initial: string, confirm: string, except?: string): Promise<string | null> {
  return askName({ title, label: "Name:", initial, confirm, validate: (n) => nameProblem(n, except) });
}

function schemeMenu(): MenuEntry[] {
  const scheme = current();
  const draft = codeStyle.draft;
  if (!scheme || !draft) return [];
  const custom = scheme.level === "ide" && scheme.id !== "default";
  const entries: MenuEntry[] = [];
  if (scheme.level === "ide" && draft.hasProject) entries.push(item("Copy to Project…", copyToProject));
  if (scheme.level === "project") {
    entries.push(
      item("Copy to IDE…", () => void ask("Copy Scheme to IDE", freeName("Project"), "OK").then((n) => n && copyAs(n))),
    );
  }
  entries.push(
    item("Duplicate…", () => void ask("Duplicate Scheme", freeName(`${scheme.name} copy`), "OK").then((n) => n && copyAs(n))),
  );
  if (custom) {
    entries.push(item("Rename…", () => void ask("Rename Scheme", scheme.name, "Rename", scheme.id).then((n) => n && renameCurrent(n))));
  }
  entries.push(item("Reset", resetCurrent));
  if (custom) entries.push(item("Delete", deleteCurrent));
  entries.push(
    separator,
    submenu(
      "Export",
      FORMATS.map((f) => item(`${f.label}…`, () => void exportScheme(f.format))),
    ),
    submenu(
      "Import Scheme",
      FORMATS.map((f) => item(`${f.label}…`, () => void importFrom(f.format))),
    ),
  );
  return entries;
}

function presetMenu(lang: CodeStyleLanguage): MenuEntry[] {
  return [submenu("Predefined Style", lang.presets.map((p) => item(p.name, () => applyPreset(lang.id, p.id))))];
}

function SchemeBar(props: { lang?: CodeStyleLanguage }) {
  const schemes = () => codeStyle.draft?.schemes ?? [];
  const ide = () => schemes().filter((s) => s.level === "ide");
  const project = () => schemes().filter((s) => s.level === "project");
  const openMenu = (e: MouseEvent, entries: MenuEntry[]) => {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    showContextMenu(entries, r.left, r.bottom + 2);
  };
  return (
    <div class="cs-scheme-bar">
      <label class="cs-scheme-label" for="cs-scheme">
        Scheme:
      </label>
      <select
        id="cs-scheme"
        class="set-input set-select cs-scheme-select"
        value={codeStyle.draft?.selected ?? ""}
        onChange={(e) => selectScheme(e.currentTarget.value)}
      >
        <optgroup label="Stored in IDE">
          <For each={ide()}>{(s) => <option value={s.id}>{s.name}</option>}</For>
        </optgroup>
        <Show when={project().length}>
          <optgroup label="Stored in Project">
            <For each={project()}>{(s) => <option value={s.id}>{s.name}</option>}</For>
          </optgroup>
        </Show>
      </select>
      <button class="icon-btn cs-gear" title="Show Scheme Actions" onClick={(e) => openMenu(e, schemeMenu())}>
        <Settings size={15} />
      </button>
      <Show when={props.lang?.presets.length}>
        <button class="set-link cs-set-from" onClick={(e) => openMenu(e, presetMenu(props.lang!))}>
          Set from…
        </button>
      </Show>
    </div>
  );
}

// ---- Options -------------------------------------------------------------------------------

function originalValue(language: string, key: string): CodeStyleValue | undefined {
  const id = codeStyle.draft?.selected;
  return codeStyle.original?.schemes.find((s) => s.id === id)?.values[`${language}.${key}`];
}

function OptionRow(props: { language: string; option: CodeStyleOption }) {
  const o = props.option;
  const value = () => valueOf(props.language, o.key) ?? o.default;
  const modified = () => {
    const before = originalValue(props.language, o.key);
    return before !== undefined && before !== value();
  };
  const set = (v: CodeStyleValue) => setValue(props.language, o.key, v);
  return (
    <div class="set-row cs-row" classList={{ modified: modified() }} data-option={`${props.language}.${o.key}`}>
      <Switch>
        <Match when={o.type === "bool"}>
          <label class="set-check">
            <input type="checkbox" checked={value() === true} onChange={(e) => set(e.currentTarget.checked)} />
            <span>{o.title}</span>
          </label>
        </Match>
        <Match when={o.type === "int" && o}>
          {(spec) => (
            <div class="set-line">
              <label class="set-label">{o.title}:</label>
              <input
                class="set-input set-number"
                type="number"
                min={spec().min}
                max={spec().max}
                value={Number(value())}
                onInput={(e) => {
                  const n = Number.parseInt(e.currentTarget.value, 10);
                  if (Number.isFinite(n)) set(Math.min(spec().max, Math.max(spec().min, n)));
                }}
              />
            </div>
          )}
        </Match>
        <Match when={o.type === "choice" && o}>
          {(spec) => (
            <div class="set-line">
              <label class="set-label">{o.title}:</label>
              <select class="set-input set-select" value={String(value())} onChange={(e) => set(e.currentTarget.value)}>
                <For each={spec().choices}>{(c) => <option value={c.value}>{c.label}</option>}</For>
              </select>
            </div>
          )}
        </Match>
        <Match when={o.type === "text"}>
          <div class="set-line">
            <label class="set-label">{o.title}:</label>
            <input class="set-input set-text" spellcheck={false} value={String(value())} onInput={(e) => set(e.currentTarget.value)} />
          </div>
        </Match>
      </Switch>
      <Show when={o.description}>
        <div class="set-desc" classList={{ "set-indent": o.type === "bool" }}>
          {o.description}
        </div>
      </Show>
    </div>
  );
}

function Options(props: { lang: CodeStyleLanguage; tab: string }) {
  const sections = createMemo(() => {
    const out: { title?: string; options: CodeStyleOption[] }[] = [];
    for (const o of props.lang.options.filter((x) => x.tab === props.tab)) {
      const last = out[out.length - 1];
      if (last && last.title === o.section) last.options.push(o);
      else out.push({ title: o.section, options: [o] });
    }
    return out;
  });
  return (
    <For each={sections()}>
      {(section) => (
        <section class="set-section">
          <Show when={section.title}>
            <div class="set-section-title">
              <span>{section.title}</span>
            </div>
          </Show>
          <div class="set-section-body" classList={{ indented: !!section.title }}>
            <For each={section.options}>{(o) => <OptionRow language={props.lang.id} option={o} />}</For>
          </div>
        </section>
      )}
    </For>
  );
}

// ---- Preview -------------------------------------------------------------------------------

function Preview(props: { language: string; path: string }) {
  let host!: HTMLDivElement;
  let view: EditorView | undefined;
  const tabSize = new Compartment();
  const [text, setText] = createSignal<string | null>(null);
  const size = () => Number(valueOf(props.language, "tabSize") ?? 4) || 4;

  // Formatted in Rust whenever an option of the language (or a general one) changes.
  const values = () => {
    const scheme = current();
    if (!scheme) return null;
    return JSON.stringify(
      Object.fromEntries(
        Object.entries(scheme.values).filter(([k]) => k.startsWith(`${props.language}.`) || k.startsWith("general.")),
      ),
    );
  };
  let seq = 0;
  let timer: ReturnType<typeof setTimeout> | undefined;
  createEffect(
    on(values, (v) => {
      if (!v) return;
      clearTimeout(timer);
      const id = ++seq;
      timer = setTimeout(() => {
        void codeStyleApi
          .preview(props.language, JSON.parse(v))
          .then((out) => id === seq && setText(out ?? ""))
          .catch((e) => console.warn("preview failed", e));
      }, 60);
    }),
  );
  onCleanup(() => clearTimeout(timer));

  onMount(() => {
    void ensureFileType(props.path).then(() => {
      view = new EditorView({
        parent: host,
        state: EditorState.create({
          doc: text() ?? "",
          extensions: [
            lineNumbers(),
            EditorState.readOnly.of(true),
            EditorView.editable.of(false),
            tabSize.of(EditorState.tabSize.of(size())),
            forgeEditorTheme,
            forgeHighlight,
            ...languageFor(props.path),
          ],
        }),
      });
    });
  });
  createEffect(() => {
    const t = text();
    const s = size();
    if (!view || t === null) return;
    view.dispatch({
      changes: t === view.state.doc.toString() ? [] : { from: 0, to: view.state.doc.length, insert: t },
      effects: tabSize.reconfigure(EditorState.tabSize.of(s)),
    });
  });
  onCleanup(() => view?.destroy());
  return <div class="cs-preview" ref={host} data-preview={props.language} />;
}

// ---- Pages ---------------------------------------------------------------------------------

/** The selected tab of each page (kept while the dialog switches pages). */
const [tabs, setTabs] = createSignal<Record<string, string>>({});

function useLoad() {
  onMount(() => void loadSchemes().catch((e) => console.warn("code style schemes could not be loaded", e)));
}

function Tabs(props: { lang: CodeStyleLanguage; tab: string }) {
  return (
    <div class="cs-tabs" role="tablist">
      <For each={props.lang.tabs}>
        {(t) => (
          <button
            class="cs-tab"
            role="tab"
            classList={{ active: props.tab === t.id }}
            data-tab={t.id}
            onClick={() => setTabs((prev) => ({ ...prev, [props.lang.id]: t.id }))}
          >
            {t.title}
          </button>
        )}
      </For>
    </div>
  );
}

const SETTING_KEYS = ["editor.codeStyle.detectIndents", "editor.codeStyle.useEditorConfig"];

/** Code Style (General): line separator, hard wrap, guides, indent detection, EditorConfig, formatter markers. */
export function GeneralPage() {
  useLoad();
  const lang = () => languageSpec("general");
  const tab = () => tabs().general ?? "general";
  return (
    <div class="set-page cs-page cs-general" data-code-style="general">
      <Show when={codeStyle.draft && lang()} fallback={<div class="set-empty">Loading…</div>}>
        {(l) => (
          <>
            <SchemeBar />
            <Tabs lang={l()} tab={tab()} />
            <div class="cs-general-body">
              <div class="cs-options">
                <Options lang={l()} tab={tab()} />
                <Show when={tab() === "general"}>
                  <section class="set-section">
                    <div class="set-section-title">
                      <span>Indents and EditorConfig</span>
                    </div>
                    <div class="set-section-body indented">
                      <For each={SETTING_KEYS.map(findSpec).filter((s) => !!s)}>{(spec) => <SettingRow spec={spec!} />}</For>
                    </div>
                  </section>
                </Show>
              </div>
              <Show when={tab() === "formatter"}>
                <Preview language="general" path={l().samplePath} />
              </Show>
            </div>
          </>
        )}
      </Show>
    </div>
  );
}

/** Code Style | <language>. */
export function LanguagePage(props: { language: string }) {
  useLoad();
  const lang = () => languageSpec(props.language);
  const tab = () => tabs()[props.language] ?? lang()?.tabs[0]?.id ?? "";
  return (
    <div class="set-page cs-page" data-code-style={props.language}>
      <Show when={codeStyle.draft && lang()} fallback={<div class="set-empty">Loading…</div>}>
        {(l) => (
          <>
            <SchemeBar lang={l()} />
            <Tabs lang={l()} tab={tab()} />
            <div class="cs-split">
              <div class="cs-options">
                <Options lang={l()} tab={tab()} />
              </div>
              <Preview language={l().id} path={l().samplePath} />
            </div>
          </>
        )}
      </Show>
    </div>
  );
}
