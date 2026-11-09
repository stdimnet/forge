/**
 * A settings page generated from the plugin settings schema: every setting whose `group`
 * is the page id, grouped by `section` under PHPStorm-style titled separators.
 */
import { createMemo, createSignal, For, Index, Match, Show, Switch, type Component, type JSX } from "solid-js";
import { Dynamic } from "solid-js/web";
import { ArrowDown, ArrowUp, Minus, Plus } from "lucide-solid";
import { settingsApi, type SettingSpec } from "@/ipc/settings";
import { settingsState } from "@/state/settings";
import { draft, draftValue, setDraft } from "./dialogState";
import { Hl, settingMatches } from "./dialogSearch";
import { t } from "@/i18n";

/** Custom editors of `objects` settings (lists of tables) and `list` settings, by setting key. */
const objectEditors = new Map<string, Component<{ spec: SettingSpec }>>();

/**
 * Register the editor of an `objects` setting, or one that replaces the plain string list
 * editor of a `list` setting; schema pages show it in the setting's row. For other types it
 * replaces the whole row (label included), e.g. a checkbox with a "Configure…" button. It
 * reads and writes the value with `draftValue` / `setDraft`, so Apply and Cancel work.
 */
export function registerObjectSettingEditor(key: string, editor: Component<{ spec: SettingSpec }>): void {
  objectEditors.set(key, editor);
}

/** Element ids of a setting's control and description (label `for`, `aria-describedby`). */
export const settingControlId = (key: string) => `setting-${key.replace(/[^\w-]/g, "-")}`;
const settingDescId = (key: string) => `${settingControlId(key)}-desc`;

/** The number typed into a numeric setting, or why it is not one (exported for tests). */
export function parseSettingNumber(raw: string, type: "int" | "number"): number | string {
  const n = type === "int" ? Number.parseInt(raw, 10) : Number.parseFloat(raw);
  return Number.isFinite(n) ? n : t("settings.validation.notANumber");
}

function NumberInput(props: { spec: SettingSpec & { type: "int" | "number" } }) {
  const step = () => (props.spec.type === "number" ? (props.spec.step ?? 0.1) : 1);
  const [invalid, setInvalid] = createSignal<string | null>(null);
  let seq = 0;
  // The schema check (type and range) runs in Rust, like the one `settings_set` applies.
  const commit = async (raw: string) => {
    const id = ++seq;
    const n = parseSettingNumber(raw, props.spec.type);
    const problem = typeof n === "string" ? n : await settingsApi.validate(props.spec.key, n);
    if (id !== seq) return;
    setInvalid(problem);
    if (!problem && typeof n === "number") setDraft(props.spec.key, n);
  };
  return (
    <input
      class="set-input set-number"
      classList={{ invalid: !!invalid() }}
      id={settingControlId(props.spec.key)}
      aria-describedby={props.spec.description ? settingDescId(props.spec.key) : undefined}
      aria-invalid={invalid() ? "true" : undefined}
      type="number"
      min={props.spec.min}
      max={props.spec.max}
      step={step()}
      value={draftValue<number>(props.spec.key)}
      onInput={(e) => void commit(e.currentTarget.value)}
      title={
        props.spec.min !== undefined && props.spec.max !== undefined
          ? `${props.spec.min} – ${props.spec.max}`
          : undefined
      }
    />
  );
}

/** Editable list of strings with a PHPStorm-like +/−/↑/↓ toolbar. */
function ListEditor(props: { spec: SettingSpec }) {
  const items = () => draftValue<string[]>(props.spec.key) ?? [];
  const [sel, setSel] = createSignal<number | null>(null);
  let listEl!: HTMLDivElement;
  const update = (next: string[]) => setDraft(props.spec.key, next);
  const add = () => {
    update([...items(), ""]);
    setSel(items().length - 1);
    queueMicrotask(() => listEl.querySelector<HTMLInputElement>("input:last-of-type")?.focus());
  };
  const remove = () => {
    const i = sel();
    if (i === null) return;
    update(items().filter((_, j) => j !== i));
    setSel(items().length ? Math.min(i, items().length - 1) : null);
  };
  const move = (d: -1 | 1) => {
    const i = sel();
    if (i === null || i + d < 0 || i + d >= items().length) return;
    const next = [...items()];
    [next[i], next[i + d]] = [next[i + d], next[i]];
    update(next);
    setSel(i + d);
  };
  return (
    <div class="set-list">
      <div class="set-list-toolbar">
        <button class="icon-btn" title={t("common.add")} onClick={add}>
          <Plus size={14} />
        </button>
        <button class="icon-btn" title={t("common.remove")} disabled={sel() === null} onClick={remove}>
          <Minus size={14} />
        </button>
        <button class="icon-btn" title={t("common.moveUp")} disabled={!sel()} onClick={() => move(-1)}>
          <ArrowUp size={14} />
        </button>
        <button
          class="icon-btn"
          title={t("common.moveDown")}
          disabled={sel() === null || sel()! >= items().length - 1}
          onClick={() => move(1)}
        >
          <ArrowDown size={14} />
        </button>
      </div>
      <div class="set-list-items" ref={listEl}>
        <Index each={items()} fallback={<div class="set-list-empty">{t("common.nothingToShow")}</div>}>
          {(item, i) => (
            <input
              class="set-list-item"
              classList={{ selected: sel() === i }}
              aria-label={`${props.spec.title} ${i + 1}`}
              value={item()}
              spellcheck={false}
              onFocus={() => setSel(i)}
              onInput={(e) => update(items().map((v, j) => (j === i ? e.currentTarget.value : v)))}
            />
          )}
        </Index>
      </div>
    </div>
  );
}

function Control(props: { spec: SettingSpec }) {
  const spec = props.spec;
  return (
    <Switch>
      <Match when={spec.type === "enum" && spec}>
        {(s) => (
          <select
            class="set-input set-select"
            id={settingControlId(spec.key)}
            aria-describedby={spec.description ? settingDescId(spec.key) : undefined}
            value={draftValue<string>(spec.key)}
            onChange={(e) => setDraft(spec.key, e.currentTarget.value)}
          >
            <For each={s().options}>{(o) => <option value={o.value}>{o.label}</option>}</For>
          </select>
        )}
      </Match>
      <Match when={(spec.type === "int" || spec.type === "number") && spec}>
        {(s) => <NumberInput spec={s() as SettingSpec & { type: "int" | "number" }} />}
      </Match>
      <Match when={spec.type === "string" || spec.type === "path"}>
        <input
          class="set-input set-text"
          classList={{ "set-path": spec.type === "path" }}
          id={settingControlId(spec.key)}
          aria-describedby={spec.description ? settingDescId(spec.key) : undefined}
          spellcheck={false}
          value={draftValue<string>(spec.key)}
          placeholder={spec.type === "path" ? t("common.default") : undefined}
          onInput={(e) => setDraft(spec.key, e.currentTarget.value)}
        />
      </Match>
    </Switch>
  );
}

export function SettingRow(props: { spec: SettingSpec }) {
  const spec = props.spec;
  const hit = () => settingMatches(spec.key);
  const modified = () => spec.key in draft;
  const description = () =>
    spec.description ? (
      <div class="set-desc" id={settingDescId(spec.key)}>
        <Hl text={spec.description} />
      </div>
    ) : null;
  return (
    <div class="set-row" classList={{ hit: hit(), modified: modified() }} data-setting={spec.key}>
      <Switch>
        <Match when={spec.type !== "list" && spec.type !== "objects" && objectEditors.get(spec.key)}>
          {(editor) => <Dynamic component={editor()} spec={spec} />}
        </Match>
        <Match when={spec.type === "bool"}>
          <label class="set-check">
            <input
              type="checkbox"
              id={settingControlId(spec.key)}
              aria-describedby={spec.description ? settingDescId(spec.key) : undefined}
              checked={draftValue<boolean>(spec.key)}
              onChange={(e) => setDraft(spec.key, e.currentTarget.checked)}
            />
            <span>
              <Hl text={spec.title} />
            </span>
          </label>
          <div class="set-indent">{description()}</div>
        </Match>
        <Match when={spec.type === "list"}>
          <div class="set-label set-label-block">
            <Hl text={spec.title} />:
          </div>
          {description()}
          <Show when={objectEditors.get(spec.key)} fallback={<ListEditor spec={spec} />}>
            {(editor) => <Dynamic component={editor()} spec={spec} />}
          </Show>
        </Match>
        <Match when={spec.type === "objects"}>
          <div class="set-label set-label-block">
            <Hl text={spec.title} />:
          </div>
          {description()}
          <Show when={objectEditors.get(spec.key)} fallback={<div class="set-desc">{t("settings.noEditor")}</div>}>
            {(editor) => <Dynamic component={editor()} spec={spec} />}
          </Show>
        </Match>
        <Match when={true}>
          <div class="set-line">
            <label class="set-label" for={settingControlId(spec.key)}>
              <Hl text={spec.title} />:
            </label>
            <Control spec={spec} />
          </div>
          {description()}
        </Match>
      </Switch>
    </div>
  );
}

export function SchemaPage(props: {
  pageId: string;
  /** Extra content at the top of a section (by section title), e.g. a detection status. */
  sectionHeader?: (section: string) => JSX.Element;
  /** Extra content at the end of a section (by section title), e.g. a Test Connection button. */
  sectionFooter?: (section: string) => JSX.Element;
}) {
  const specs = createMemo(() =>
    settingsState.schema.filter((s) => s.group === props.pageId && !s.hidden).sort((a, b) => a.order - b.order),
  );
  /** Consecutive runs of settings with the same section, in schema order. */
  const sections = createMemo(() => {
    const out: { title?: string; specs: SettingSpec[] }[] = [];
    for (const s of specs()) {
      const last = out[out.length - 1];
      if (last && last.title === s.section) last.specs.push(s);
      else out.push({ title: s.section, specs: [s] });
    }
    return out;
  });
  return (
    <div class="set-page">
      <Show
        when={specs().length}
        fallback={
          <div class="set-empty">
            {settingsState.loaded
              ? t("tools.settings.noSettingsToShowThe")
              : t("common.loadingEllipsis")}
          </div>
        }
      >
        <For each={sections()}>
          {(section) => (
            <section class="set-section">
              <Show when={section.title}>
                <div class="set-section-title">
                  <span>
                    <Hl text={section.title!} />
                  </span>
                </div>
              </Show>
              <div class="set-section-body" classList={{ indented: !!section.title }}>
                <Show when={section.title ? props.sectionHeader : undefined}>{(header) => header()(section.title!)}</Show>
                <For each={section.specs}>{(spec) => <SettingRow spec={spec} />}</For>
                <Show when={section.title ? props.sectionFooter : undefined}>{(footer) => footer()(section.title!)}</Show>
              </div>
            </section>
          )}
        </For>
      </Show>
    </div>
  );
}
